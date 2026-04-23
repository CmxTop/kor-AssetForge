package handlers

import (
	"encoding/json"
	"fmt"
	"net/http"
	"sync"
	"sync/atomic"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/yourusername/kor-assetforge/utils"
	"go.uber.org/zap"
)

// EventType classifies a real-time WebSocket event.
type EventType string

const (
	EventAssetCreated     EventType = "asset.created"
	EventAssetUpdated     EventType = "asset.updated"
	EventTransactionNew   EventType = "transaction.new"
	EventListingCreated   EventType = "listing.created"
	EventMarketplaceTrade EventType = "marketplace.trade"
	EventPing             EventType = "ping"
)

// Topic is a named channel that subscribers can filter on.
type Topic string

const (
	TopicAssets       Topic = "assets"
	TopicTransactions Topic = "transactions"
	TopicMarketplace  Topic = "marketplace"
	TopicAll          Topic = "all"
)

// WSEvent is the envelope sent to every client.
type WSEvent struct {
	Type      EventType   `json:"type"`
	Payload   interface{} `json:"payload"`
	Timestamp time.Time   `json:"timestamp"`
}

// WSStats exposes live metrics about the hub.
type WSStats struct {
	ConnectedClients int64     `json:"connected_clients"`
	TotalBroadcasts  int64     `json:"total_broadcasts"`
	Uptime           time.Time `json:"uptime"`
}

// ---- subscriber -------------------------------------------------------------

type subscriber struct {
	id     string
	conn   *utils.WSConn
	topics map[Topic]bool
	send   chan []byte
}

func newSubscriber(id string, conn *utils.WSConn, topics []string) *subscriber {
	t := make(map[Topic]bool)
	if len(topics) == 0 {
		t[TopicAll] = true
	}
	for _, name := range topics {
		t[Topic(name)] = true
	}
	return &subscriber{id: id, conn: conn, topics: t, send: make(chan []byte, 128)}
}

func (s *subscriber) matches(topic Topic) bool {
	return s.topics[TopicAll] || s.topics[topic]
}

// ---- singleflight (stdlib-only stampede guard used by broadcast) ------------

// ---- Hub --------------------------------------------------------------------

type hubMsg struct {
	topic   Topic
	payload []byte
}

// Hub manages all active WebSocket connections and fan-out broadcasting.
type Hub struct {
	mu          sync.RWMutex
	subscribers map[string]*subscriber

	register   chan *subscriber
	unregister chan string
	broadcast  chan *hubMsg

	totalBroadcasts atomic.Int64
	startedAt       time.Time
}

var (
	globalHub  *Hub
	hubOnce    sync.Once
)

// GetHub returns the process-wide singleton Hub, starting its run loop once.
func GetHub() *Hub {
	hubOnce.Do(func() {
		globalHub = &Hub{
			subscribers: make(map[string]*subscriber),
			register:    make(chan *subscriber, 64),
			unregister:  make(chan string, 64),
			broadcast:   make(chan *hubMsg, 512),
			startedAt:   time.Now(),
		}
		go globalHub.run()
	})
	return globalHub
}

func (h *Hub) run() {
	heartbeat := time.NewTicker(30 * time.Second)
	defer heartbeat.Stop()

	for {
		select {
		case sub := <-h.register:
			h.mu.Lock()
			h.subscribers[sub.id] = sub
			h.mu.Unlock()
			Logger.Info("WebSocket client connected",
				zap.String("id", sub.id),
				zap.Int("total", h.connectedCount()))
			// Start the writer goroutine for this subscriber
			go h.writerLoop(sub)

		case id := <-h.unregister:
			h.mu.Lock()
			if sub, ok := h.subscribers[id]; ok {
				sub.conn.Close()
				close(sub.send)
				delete(h.subscribers, id)
			}
			h.mu.Unlock()
			Logger.Info("WebSocket client disconnected",
				zap.String("id", id),
				zap.Int("total", h.connectedCount()))

		case msg := <-h.broadcast:
			h.totalBroadcasts.Add(1)
			h.fanOut(msg)

		case <-heartbeat.C:
			h.sendHeartbeat()
		}
	}
}

func (h *Hub) fanOut(msg *hubMsg) {
	h.mu.RLock()
	defer h.mu.RUnlock()
	for id, sub := range h.subscribers {
		if !sub.matches(msg.topic) {
			continue
		}
		select {
		case sub.send <- msg.payload:
		default:
			// Slow client: schedule disconnect to avoid blocking the hub
			go func(cid string) { h.unregister <- cid }(id)
		}
	}
}

func (h *Hub) sendHeartbeat() {
	evt := WSEvent{Type: EventPing, Payload: nil, Timestamp: time.Now()}
	data, _ := json.Marshal(evt)
	h.mu.RLock()
	defer h.mu.RUnlock()
	for _, sub := range h.subscribers {
		select {
		case sub.send <- data:
		default:
		}
	}
}

func (h *Hub) writerLoop(sub *subscriber) {
	for msg := range sub.send {
		if err := sub.conn.WriteMessage(msg); err != nil {
			h.unregister <- sub.id
			return
		}
	}
}

func (h *Hub) connectedCount() int {
	h.mu.RLock()
	defer h.mu.RUnlock()
	return len(h.subscribers)
}

// Broadcast dispatches an event to all subscribers interested in topic.
func (h *Hub) Broadcast(topic Topic, event WSEvent) {
	data, err := json.Marshal(event)
	if err != nil {
		return
	}
	select {
	case h.broadcast <- &hubMsg{topic: topic, payload: data}:
	default:
		Logger.Warn("WebSocket broadcast channel full, dropping message")
	}
}

// BroadcastEvent is the package-level convenience helper.
func BroadcastEvent(topic Topic, evType EventType, payload interface{}) {
	GetHub().Broadcast(topic, WSEvent{
		Type:      evType,
		Payload:   payload,
		Timestamp: time.Now(),
	})
}

// ---- HTTP handler -----------------------------------------------------------

// WebSocketHandler wires WebSocket endpoints into Gin.
type WebSocketHandler struct{}

func NewWebSocketHandler() *WebSocketHandler { return &WebSocketHandler{} }

// HandleWS upgrades GET /ws to a WebSocket connection.
//
// Query parameters:
//
//	token  – bearer token for authentication (validated below)
//	topics – repeated; one of: assets, transactions, marketplace, all (default)
func (wsh *WebSocketHandler) HandleWS(c *gin.Context) {
	// Lightweight auth: accept any non-empty token; production would verify JWT.
	token := c.Query("token")
	if token == "" {
		token = c.GetHeader("Authorization")
	}
	if token == "" {
		c.JSON(http.StatusUnauthorized, gin.H{"error": "authentication token required"})
		return
	}

	topicParams := c.QueryArray("topics")

	wsConn, err := utils.UpgradeHTTP(c.Writer, c.Request)
	if err != nil {
		// UpgradeHTTP already hijacked the socket on success; on failure the
		// HTTP response is still writable.
		c.JSON(http.StatusBadRequest, gin.H{"error": "WebSocket upgrade failed: " + err.Error()})
		return
	}

	reqID := c.GetString("request_id")
	if reqID == "" {
		reqID = fmt.Sprintf("ws-%d", time.Now().UnixNano())
	}

	sub := newSubscriber(reqID, wsConn, topicParams)
	hub := GetHub()
	hub.register <- sub

	// Reader loop: handles ping/pong, close frames, and client commands.
	for {
		op, msg, err := wsConn.ReadMessage()
		if err != nil {
			break
		}
		switch op {
		case utils.OpClose:
			hub.unregister <- sub.id
			return
		case utils.OpPing:
			_ = wsConn.WritePong(msg)
		case utils.OpText:
			// Client can send {"action":"subscribe","topic":"assets"} etc.
			wsh.handleClientMessage(sub, msg)
		}
	}

	hub.unregister <- sub.id
}

// handleClientMessage processes JSON commands sent by the client.
func (wsh *WebSocketHandler) handleClientMessage(sub *subscriber, raw []byte) {
	var cmd struct {
		Action string `json:"action"`
		Topic  string `json:"topic"`
	}
	if err := json.Unmarshal(raw, &cmd); err != nil {
		return
	}
	switch cmd.Action {
	case "subscribe":
		sub.topics[Topic(cmd.Topic)] = true
	case "unsubscribe":
		delete(sub.topics, Topic(cmd.Topic))
	}
}

// HandleWSStats returns live hub metrics.
func (wsh *WebSocketHandler) HandleWSStats(c *gin.Context) {
	hub := GetHub()
	c.JSON(http.StatusOK, WSStats{
		ConnectedClients: int64(hub.connectedCount()),
		TotalBroadcasts:  hub.totalBroadcasts.Load(),
		Uptime:           hub.startedAt,
	})
}
