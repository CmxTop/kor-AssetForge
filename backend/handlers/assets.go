package handlers

import (
	"encoding/json"
	"net/http"
	"strconv"

	"github.com/gin-gonic/gin"
	"github.com/yourusername/kor-assetforge/models"
	"github.com/yourusername/kor-assetforge/utils"
	"gorm.io/gorm"
)

type AssetHandler struct {
	db            *gorm.DB
	stellarClient *utils.StellarClient
}

func NewAssetHandler(db *gorm.DB, stellarClient *utils.StellarClient) *AssetHandler {
	return &AssetHandler{
		db:            db,
		stellarClient: stellarClient,
	}
}

// TokenizeAsset handles formal asset tokenization with Soroban integration
func (h *AssetHandler) TokenizeAsset(c *gin.Context) {
	var req struct {
		IssuerAccount string            `json:"issuer_account" binding:"required"`
		Name          string            `json:"name" binding:"required"`
		Symbol        string            `json:"symbol" binding:"required"`
		Description   string            `json:"description"`
		AssetType     string            `json:"asset_type" binding:"required"`
		TotalSupply   int64             `json:"total_supply" binding:"required,gt=0"`
		Metadata      map[string]string `json:"metadata"`
		Fractions     uint64            `json:"fractions"`
	}

	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	// Validate Stellar address
	if err := h.stellarClient.ValidateAddress(req.IssuerAccount); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "Invalid issuer account address"})
		return
	}

	// Marshal metadata to JSON string
	metadataJSON, _ := json.Marshal(req.Metadata)

	// Create record in database
	asset := models.Asset{
		Name:         req.Name,
		Symbol:       req.Symbol,
		Description:  req.Description,
		AssetType:    req.AssetType,
		TotalSupply:  req.TotalSupply,
		Fractions:    req.Fractions,
		OwnerAddress: req.IssuerAccount,
		Metadata:     string(metadataJSON),
		Verified:     false,
	}

	if err := h.db.Create(&asset).Error; err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "Failed to create asset record"})
		return
	}

	// Invoke Soroban contract to mint tokens
	// params: [asset_name, symbol, total_supply, issuer]
	params := []interface{}{req.Name, req.Symbol, req.TotalSupply, req.IssuerAccount}
	
	// TODO: Get contract ID from config or dynamic deployment
	contractID := "CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"
	
	txHash, err := h.stellarClient.InvokeContract(contractID, "mint", params)
	if err != nil {
		// Log error but the DB record is already created with verified=false
		// In a production app, we might want to roll back or mark as failed
		c.JSON(http.StatusAccepted, gin.H{
			"message": "Asset created in database but contract invocation failed",
			"asset":   asset,
			"error":   err.Error(),
		})
		return
	}

	// Update asset with contract ID and status if successful
	h.db.Model(&asset).Update("verified", true)

	c.JSON(http.StatusCreated, gin.H{
		"message": "Asset tokenized successfully",
		"asset":   asset,
		"tx_hash": txHash,
	})
}

// ListAssets returns all assets with pagination
func (h *AssetHandler) ListAssets(c *gin.Context) {
	var assets []models.Asset
	var total int64
	page, limit := utils.GetPaginationParams(c)

	if err := utils.Paginate(h.db, page, limit, &total, &assets); err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "Failed to fetch assets"})
		return
	}

	c.JSON(http.StatusOK, utils.Pagination{
		Limit: limit,
		Page:  page,
		Total: total,
		Data:  assets,
	})
}

// ListTransactions returns all transactions with pagination
func (h *AssetHandler) ListTransactions(c *gin.Context) {
	var transactions []models.Transaction
	var total int64
	page, limit := utils.GetPaginationParams(c)

	// Build query (allow filtering by asset_id if provided)
	query := h.db.Model(&models.Transaction{}).Order("created_at desc")
	if assetID := c.Query("asset_id"); assetID != "" {
		query = query.Where("asset_id = ?", assetID)
	}

	if err := utils.Paginate(query, page, limit, &total, &transactions); err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "Failed to fetch transactions"})
		return
	}

	c.JSON(http.StatusOK, utils.Pagination{
		Limit: limit,
		Page:  page,
		Total: total,
		Data:  transactions,
	})
}

// GetAsset returns a specific asset
func (h *AssetHandler) GetAsset(c *gin.Context) {
	id, err := strconv.ParseUint(c.Param("id"), 10, 32)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "Invalid asset ID"})
		return
	}

	var asset models.Asset
	if err := h.db.First(&asset, id).Error; err != nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "Asset not found"})
		return
	}

	c.JSON(http.StatusOK, asset)
}

// ListAssetForSale creates a marketplace listing
func (h *AssetHandler) ListAssetForSale(c *gin.Context) {
	var req struct {
		AssetID      uint   `json:"asset_id" binding:"required"`
		SellerAddr   string `json:"seller_address" binding:"required"`
		Amount       int64  `json:"amount" binding:"required,gt=0"`
		PricePerUnit int64  `json:"price_per_unit" binding:"required,gt=0"`
	}

	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	// TODO: Create on-chain listing and get listing ID
	listingID := "listing_1"

	listing := models.Listing{
		AssetID:      req.AssetID,
		SellerAddr:   req.SellerAddr,
		Amount:       req.Amount,
		PricePerUnit: req.PricePerUnit,
		Active:       true,
		ListingID:    listingID,
	}

	if err := h.db.Create(&listing).Error; err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "Failed to create listing"})
		return
	}

	c.JSON(http.StatusCreated, listing)
}

// TransferAsset handles asset transfers
func (h *AssetHandler) TransferAsset(c *gin.Context) {
	var req struct {
		AssetID     uint   `json:"asset_id" binding:"required"`
		FromAddress string `json:"from_address" binding:"required"`
		ToAddress   string `json:"to_address" binding:"required"`
		Amount      int64  `json:"amount" binding:"required,gt=0"`
	}

	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	// TODO: Execute on-chain transfer
	txHash := "tx_hash_placeholder"

	transaction := models.Transaction{
		AssetID:     req.AssetID,
		FromAddress: req.FromAddress,
		ToAddress:   req.ToAddress,
		Amount:      req.Amount,
		TxHash:      txHash,
		Status:      "pending",
	}

	if err := h.db.Create(&transaction).Error; err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "Failed to record transaction"})
		return
	}

	c.JSON(http.StatusOK, transaction)
}
