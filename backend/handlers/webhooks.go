package handlers

import (
	"bytes"
	"crypto/hmac"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"io"
	"log"
	"net/http"
	"os"

	"github.com/gin-gonic/gin"
	"github.com/yourusername/kor-assetforge/models"
	"github.com/yourusername/kor-assetforge/validator"
	"gorm.io/gorm"
)

type WebhookHandler struct {
	db *gorm.DB
}

func NewWebhookHandler(db *gorm.DB) *WebhookHandler {
	return &WebhookHandler{db: db}
}

// StellarEvent represents the payload from Stellar Horizon events
type StellarEvent struct {
	ID          string          `json:"id" binding:"required,no_html"`
	PagingToken string          `json:"paging_token" binding:"required,no_html"`
	Type        string          `json:"type" binding:"required,no_html"`
	ContractID  string          `json:"contract_id" binding:"required,no_html"`
	Topic       []string        `json:"topic" binding:"required,dive,no_html"`
	Value       json.RawMessage `json:"value" binding:"required"`
	Ledger      int32           `json:"ledger" binding:"required"`
	CreatedAt   string          `json:"created_at" binding:"required,no_html"`
}

// HandleStellarEvent processes events received from Stellar Horizon
// @Summary Handle Stellar events
// @Description Webhook endpoint for receiving and processing events from the Stellar network
// @Tags webhooks
// @Accept json
// @Produce json
// @Param X-Stellar-Signature header string false "HMAC signature for verification"
// @Param event body StellarEvent true "Stellar event payload"
// @Success 200 {object} map[string]interface{}
// @Failure 401 {object} map[string]interface{}
// @Router /webhooks/stellar-events [post]
func (h *WebhookHandler) HandleStellarEvent(c *gin.Context) {
	// 1. Verify Signature
	signature := c.GetHeader("X-Stellar-Signature")
	secret := os.Getenv("WEBHOOK_SECRET")

	if secret != "" && signature != "" {
		body, _ := io.ReadAll(c.Request.Body)
		if !verifySignature(body, signature, secret) {
			c.JSON(http.StatusUnauthorized, gin.H{"error": "Invalid signature"})
			return
		}
		// Reset body for binding
		c.Request.Body = io.NopCloser(bytes.NewReader(body))
	}

	var event StellarEvent
	if err := c.ShouldBindJSON(&event); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "Invalid event payload"})
		return
	}

	validator.SanitizeStruct(&event)

	// 2. Idempotency Check
	// In a real app, we would store event IDs in a separate table
	log.Printf("Processing Stellar event: %s (Type: %s)", event.ID, event.Type)

	// 3. Parse and Process Event
	// Filter for Soroban events (topics are used to identify the event type)
	if len(event.Topic) > 0 {
		eventType := event.Topic[0]
		
		switch eventType {
		case "minted":
			h.processMintEvent(event)
		case "listed":
			h.processListingEvent(event)
		case "transfer":
			h.processTransferEvent(event)
		default:
			log.Printf("Ignoring unknown event type: %s", eventType)
		}
	}

	c.JSON(http.StatusOK, gin.H{"status": "processed", "event_id": event.ID})
}

func (h *WebhookHandler) processMintEvent(event StellarEvent) {
	// Sync asset status in DB
	// Extract asset ID or symbol from topic/value
	log.Printf("Syncing asset status for contract: %s", event.ContractID)
	
	var asset models.Asset
	if err := h.db.Where("contract_id = ?", event.ContractID).First(&asset).Error; err == nil {
		h.db.Model(&asset).Update("verified", true)
		log.Printf("Asset %s marked as verified on-chain", asset.Symbol)
	}
}

func (h *WebhookHandler) processListingEvent(event StellarEvent) {
	log.Printf("New marketplace listing detected on-chain for contract: %s", event.ContractID)
}

func (h *WebhookHandler) processTransferEvent(event StellarEvent) {
	log.Printf("Asset transfer detected on-chain for contract: %s", event.ContractID)
}

func verifySignature(payload []byte, signature, secret string) bool {
	h := hmac.New(sha256.New, []byte(secret))
	h.Write(payload)
	expectedSignature := hex.EncodeToString(h.Sum(nil))
	return hmac.Equal([]byte(signature), []byte(expectedSignature))
}
