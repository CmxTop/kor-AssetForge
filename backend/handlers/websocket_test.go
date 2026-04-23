package handlers

import (
	"encoding/json"
	"sync"
	"testing"
	"time"
)

// mockWSConn is a test double for utils.WSConn that records written messages.
type mockWSConn struct {
	mu       sync.Mutex
	messages [][]byte
	closed   bool
}

func (m *mockWSConn) WriteMessage(msg []byte) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	if m.closed {
		return nil
	}
	m.messages = append(m.messages, msg)
	return nil
}

func (m *mockWSConn) Close() {
	m.mu.Lock()
	m.closed = true
	m.mu.Unlock()
}

func (m *mockWSConn) WrittenMessages() [][]byte {
	m.mu.Lock()
	defer m.mu.Unlock()
	cp := make([][]byte, len(m.messages))
	copy(cp, m.messages)
	return cp
}

// ---- tests ------------------------------------------------------------------

func TestHub_RegisterAndBroadcast(t *testing.T) {
	// Use a fresh hub so tests are isolated.
	hub := &Hub{
		subscribers: make(map[string]*subscriber),
		register:    make(chan *subscriber, 8),
		unregister:  make(chan string, 8),
		broadcast:   make(chan *hubMsg, 64),
		startedAt:   time.Now(),
	}
	go hub.run()

	received := make(chan []byte, 4)

	// Build a subscriber whose send channel we watch directly.
	sub := &subscriber{
		id:     "test-client-1",
		topics: map[Topic]bool{TopicAll: true},
		send:   make(chan []byte, 16),
	}
	hub.register <- sub

	// Drain the send channel into received.
	go func() {
		for msg := range sub.send {
			received <- msg
		}
	}()

	// Give the hub goroutine a moment to register the subscriber.
	time.Sleep(10 * time.Millisecond)

	evt := WSEvent{
		Type:      EventAssetCreated,
		Payload:   map[string]interface{}{"id": 42},
		Timestamp: time.Now(),
	}
	hub.Broadcast(TopicAssets, evt)

	select {
	case raw := <-received:
		var got WSEvent
		if err := json.Unmarshal(raw, &got); err != nil {
			t.Fatalf("unmarshal error: %v", err)
		}
		if got.Type != EventAssetCreated {
			t.Errorf("expected type %s, got %s", EventAssetCreated, got.Type)
		}
	case <-time.After(200 * time.Millisecond):
		t.Fatal("timed out waiting for broadcast")
	}
}

func TestHub_TopicFiltering(t *testing.T) {
	hub := &Hub{
		subscribers: make(map[string]*subscriber),
		register:    make(chan *subscriber, 8),
		unregister:  make(chan string, 8),
		broadcast:   make(chan *hubMsg, 64),
		startedAt:   time.Now(),
	}
	go hub.run()

	// sub1 listens only to transactions
	sub1 := &subscriber{
		id:     "tx-listener",
		topics: map[Topic]bool{TopicTransactions: true},
		send:   make(chan []byte, 16),
	}
	// sub2 listens only to assets
	sub2 := &subscriber{
		id:     "asset-listener",
		topics: map[Topic]bool{TopicAssets: true},
		send:   make(chan []byte, 16),
	}
	hub.register <- sub1
	hub.register <- sub2
	time.Sleep(10 * time.Millisecond)

	// Broadcast an asset event — only sub2 should receive it.
	hub.Broadcast(TopicAssets, WSEvent{Type: EventAssetCreated, Timestamp: time.Now()})
	time.Sleep(20 * time.Millisecond)

	if len(sub1.send) != 0 {
		t.Error("transaction-only subscriber should NOT receive asset events")
	}
	if len(sub2.send) != 1 {
		t.Errorf("asset subscriber should receive exactly 1 event, got %d", len(sub2.send))
	}
}

func TestHub_Unregister(t *testing.T) {
	hub := &Hub{
		subscribers: make(map[string]*subscriber),
		register:    make(chan *subscriber, 8),
		unregister:  make(chan string, 8),
		broadcast:   make(chan *hubMsg, 64),
		startedAt:   time.Now(),
	}
	go hub.run()

	sub := &subscriber{
		id:     "leave-me",
		topics: map[Topic]bool{TopicAll: true},
		send:   make(chan []byte, 16),
	}
	hub.register <- sub
	time.Sleep(10 * time.Millisecond)

	if hub.connectedCount() != 1 {
		t.Fatalf("expected 1 connected client, got %d", hub.connectedCount())
	}

	hub.unregister <- sub.id
	time.Sleep(10 * time.Millisecond)

	if hub.connectedCount() != 0 {
		t.Errorf("expected 0 connected clients after unregister, got %d", hub.connectedCount())
	}
}

func TestHub_MultipleSubscribers(t *testing.T) {
	hub := &Hub{
		subscribers: make(map[string]*subscriber),
		register:    make(chan *subscriber, 16),
		unregister:  make(chan string, 16),
		broadcast:   make(chan *hubMsg, 64),
		startedAt:   time.Now(),
	}
	go hub.run()

	const n = 5
	subs := make([]*subscriber, n)
	for i := 0; i < n; i++ {
		s := &subscriber{
			id:     "client-" + string(rune('0'+i)),
			topics: map[Topic]bool{TopicAll: true},
			send:   make(chan []byte, 16),
		}
		subs[i] = s
		hub.register <- s
	}
	time.Sleep(20 * time.Millisecond)

	hub.Broadcast(TopicMarketplace, WSEvent{Type: EventMarketplaceTrade, Timestamp: time.Now()})
	time.Sleep(20 * time.Millisecond)

	for _, s := range subs {
		if len(s.send) != 1 {
			t.Errorf("subscriber %s: expected 1 message, got %d", s.id, len(s.send))
		}
	}
}

func TestWSEvent_JSONShape(t *testing.T) {
	evt := WSEvent{
		Type:      EventTransactionNew,
		Payload:   map[string]int{"asset_id": 7, "amount": 100},
		Timestamp: time.Now(),
	}
	data, err := json.Marshal(evt)
	if err != nil {
		t.Fatalf("marshal error: %v", err)
	}

	var out map[string]interface{}
	if err := json.Unmarshal(data, &out); err != nil {
		t.Fatalf("unmarshal error: %v", err)
	}
	if out["type"] != string(EventTransactionNew) {
		t.Errorf("unexpected type: %v", out["type"])
	}
	if out["payload"] == nil {
		t.Error("payload should not be nil")
	}
}

func TestSubscriber_Matches(t *testing.T) {
	cases := []struct {
		topics map[Topic]bool
		topic  Topic
		want   bool
	}{
		{map[Topic]bool{TopicAll: true}, TopicAssets, true},
		{map[Topic]bool{TopicAssets: true}, TopicAssets, true},
		{map[Topic]bool{TopicTransactions: true}, TopicAssets, false},
		{map[Topic]bool{TopicAll: true}, TopicMarketplace, true},
	}

	for _, tc := range cases {
		s := &subscriber{topics: tc.topics}
		if got := s.matches(tc.topic); got != tc.want {
			t.Errorf("topics=%v, topic=%s: matches()=%v, want %v",
				tc.topics, tc.topic, got, tc.want)
		}
	}
}
