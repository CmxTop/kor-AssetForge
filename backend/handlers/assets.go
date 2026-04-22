package handlers

import (
	"context"
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"strconv"
	"strings"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/redis/go-redis/v9"
	"github.com/yourusername/kor-assetforge/models"
	"github.com/yourusername/kor-assetforge/utils"
	"github.com/yourusername/kor-assetforge/validator"
	"gorm.io/gorm"
)

type AssetHandler struct {
	db            *gorm.DB
	stellarClient *utils.StellarClient
	redisClient   *redis.Client
}

func NewAssetHandler(db *gorm.DB, stellarClient *utils.StellarClient, redisClient *redis.Client) *AssetHandler {
	return &AssetHandler{
		db:            db,
		stellarClient: stellarClient,
		redisClient:   redisClient,
	}
}

// TokenizeAsset handles formal asset tokenization with Soroban integration
func (h *AssetHandler) TokenizeAsset(c *gin.Context) {
	var req validator.TokenizeAssetRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	validator.SanitizeStruct(&req)
	req.Symbol = strings.ToUpper(req.Symbol)

	if err := validator.ValidateStruct(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	metadataJSON, _ := json.Marshal(req.Metadata)

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

	if h.redisClient != nil {
		ctx := context.Background()
		if err := h.redisClient.Del(ctx, "kor:asset:list:page1").Err(); err != nil {
			log.Printf("Warning: failed to invalidate cache for list: %v", err)
		}
	}

	params := []interface{}{req.Name, req.Symbol, req.TotalSupply, req.IssuerAccount}
	contractID := "CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"

	txHash, err := h.stellarClient.InvokeContract(contractID, "mint", params)
	if err != nil {
		c.JSON(http.StatusAccepted, gin.H{
			"message": "Asset created in database but contract invocation failed",
			"asset":   asset,
			"error":   err.Error(),
		})
		return
	}

	h.db.Model(&asset).Update("verified", true)

	c.JSON(http.StatusCreated, gin.H{
		"message": "Asset tokenized successfully",
		"asset":   asset,
		"tx_hash": txHash,
	})
}

// ListAssets returns all assets with pagination
func (h *AssetHandler) ListAssets(c *gin.Context) {
	cacheKey := "kor:asset:list:page1"

	// Try fetching from Redis first
	if h.redisClient != nil {
		ctx := context.Background()
		cachedData, err := h.redisClient.Get(ctx, cacheKey).Result()
		if err == nil {
			log.Printf("Cache hit for %s", cacheKey)
			c.Data(http.StatusOK, "application/json", []byte(cachedData))
			return
		} else if err != redis.Nil {
			log.Printf("Redis error on Get %s: %v", cacheKey, err)
		}
	}

	var query validator.PaginationQuery
	if err := c.ShouldBindQuery(&query); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	page := query.Page
	if page == 0 {
		page = 1
	}
	limit := query.Limit
	if limit == 0 {
		limit = 10
	}

	var assets []models.Asset
	var total int64
	if err := utils.Paginate(h.db, page, limit, &total, &assets); err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "Failed to fetch assets"})
		return
	}

	paginationRes := utils.Pagination{
		Limit: limit,
		Page:  page,
		Total: total,
		Data:  assets,
	}

	// Save to Redis (simplified: only cache page 1 default view for now to match upstream)
	if h.redisClient != nil && page == 1 {
		if jsonData, err := json.Marshal(paginationRes); err == nil {
			ctx := context.Background()
			if err := h.redisClient.Set(ctx, cacheKey, jsonData, 5*time.Minute).Err(); err != nil {
				log.Printf("Warning: failed to cache list: %v", err)
			}
		}
	}

	c.JSON(http.StatusOK, paginationRes)
}

// ListTransactions returns all transactions with pagination
func (h *AssetHandler) ListTransactions(c *gin.Context) {
	var queryParams validator.TransactionQuery
	if err := c.ShouldBindQuery(&queryParams); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	page := queryParams.Page
	if page == 0 {
		page = 1
	}
	limit := queryParams.Limit
	if limit == 0 {
		limit = 10
	}

	var transactions []models.Transaction
	var total int64
	query := h.db.Model(&models.Transaction{}).Order("created_at desc")
	if queryParams.AssetID != 0 {
		query = query.Where("asset_id = ?", queryParams.AssetID)
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
	var uri validator.AssetIDUri
	if err := c.ShouldBindUri(&uri); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "Invalid asset ID"})
		return
	}

	cacheKey := fmt.Sprintf("kor:asset:detail:%d", uri.ID)

	// Try fetching from Redis first
	if h.redisClient != nil {
		ctx := context.Background()
		cachedData, err := h.redisClient.Get(ctx, cacheKey).Result()
		if err == nil {
			log.Printf("Cache hit for %s", cacheKey)
			c.Data(http.StatusOK, "application/json", []byte(cachedData))
			return
		} else if err != redis.Nil {
			log.Printf("Redis error on Get %s: %v", cacheKey, err)
		}
	}

	var asset models.Asset
	if err := h.db.First(&asset, uri.ID).Error; err != nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "Asset not found"})
		return
	}

	// Save to Redis
	if h.redisClient != nil {
		if jsonData, err := json.Marshal(asset); err == nil {
			ctx := context.Background()
			if err := h.redisClient.Set(ctx, cacheKey, jsonData, 5*time.Minute).Err(); err != nil {
				log.Printf("Warning: failed to cache detail for %d: %v", uri.ID, err)
			}
		}
	}

	c.JSON(http.StatusOK, asset)
}

// ListAssetForSale creates a marketplace listing
func (h *AssetHandler) ListAssetForSale(c *gin.Context) {
	var req validator.ListAssetSaleRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	validator.SanitizeStruct(&req)
	if err := validator.ValidateStruct(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	// Ensure asset exists before creating a listing
	var asset models.Asset
	if err := h.db.First(&asset, req.AssetID).Error; err != nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "Asset not found"})
		return
	}

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

	if h.redisClient != nil {
		ctx := context.Background()
		detailKey := fmt.Sprintf("kor:asset:detail:%d", req.AssetID)
		if err := h.redisClient.Del(ctx, detailKey).Err(); err != nil {
			log.Printf("Warning: failed to invalidate cache for asset %d: %v", req.AssetID, err)
		}
	}

	c.JSON(http.StatusCreated, listing)
}

// TransferAsset handles asset transfers
func (h *AssetHandler) TransferAsset(c *gin.Context) {
	var req validator.TransferAssetRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	validator.SanitizeStruct(&req)
	if err := validator.ValidateStruct(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	// Ensure asset exists before recording the transfer
	var asset models.Asset
	if err := h.db.First(&asset, req.AssetID).Error; err != nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "Asset not found"})
		return
	}

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

	if h.redisClient != nil {
		ctx := context.Background()
		detailKey := fmt.Sprintf("kor:asset:detail:%d", req.AssetID)
		if err := h.redisClient.Del(ctx, detailKey).Err(); err != nil {
			log.Printf("Warning: failed to invalidate cache for asset %d: %v", req.AssetID, err)
		}
	}

	c.JSON(http.StatusOK, transaction)
}
