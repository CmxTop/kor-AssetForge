package handlers

import (
	"context"
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"strconv"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/redis/go-redis/v9"
	"github.com/yourusername/kor-assetforge/models"
	"github.com/yourusername/kor-assetforge/utils"
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

// CreateAsset handles asset tokenization
func (h *AssetHandler) CreateAsset(c *gin.Context) {
	var req struct {
		Name         string `json:"name" binding:"required"`
		Symbol       string `json:"symbol" binding:"required"`
		Description  string `json:"description"`
		AssetType    string `json:"asset_type" binding:"required"`
		TotalSupply  int64  `json:"total_supply" binding:"required,gt=0"`
		OwnerAddress string `json:"owner_address" binding:"required"`
		ImageURL     string `json:"image_url"`
		DocumentURL  string `json:"document_url"`
	}

	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	// TODO: Deploy smart contract and get contract ID
	contractID := "CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"

	asset := models.Asset{
		Name:         req.Name,
		Symbol:       req.Symbol,
		Description:  req.Description,
		AssetType:    req.AssetType,
		TotalSupply:  req.TotalSupply,
		ContractID:   contractID,
		OwnerAddress: req.OwnerAddress,
		ImageURL:     req.ImageURL,
		DocumentURL:  req.DocumentURL,
		Verified:     false,
	}

	if err := h.db.Create(&asset).Error; err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "Failed to create asset"})
		return
	}

	// Invalidate list cache
	if h.redisClient != nil {
		ctx := context.Background()
		if err := h.redisClient.Del(ctx, "kor:asset:list:page1").Err(); err != nil {
			log.Printf("Warning: failed to invalidate cache for list: %v", err)
		}
	}

	c.JSON(http.StatusCreated, asset)
}

// ListAssets returns all assets
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

	var assets []models.Asset

	if err := h.db.Find(&assets).Error; err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "Failed to fetch assets"})
		return
	}

	// Save to Redis
	if h.redisClient != nil {
		if jsonData, err := json.Marshal(assets); err == nil {
			ctx := context.Background()
			if err := h.redisClient.Set(ctx, cacheKey, jsonData, 5*time.Minute).Err(); err != nil {
				log.Printf("Warning: failed to cache list: %v", err)
			}
		}
	}

	c.JSON(http.StatusOK, assets)
}

// GetAsset returns a specific asset
func (h *AssetHandler) GetAsset(c *gin.Context) {
	idStr := c.Param("id")
	id, err := strconv.ParseUint(idStr, 10, 32)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "Invalid asset ID"})
		return
	}

	cacheKey := fmt.Sprintf("kor:asset:detail:%d", id)

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
	if err := h.db.First(&asset, id).Error; err != nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "Asset not found"})
		return
	}

	// Save to Redis
	if h.redisClient != nil {
		if jsonData, err := json.Marshal(asset); err == nil {
			ctx := context.Background()
			if err := h.redisClient.Set(ctx, cacheKey, jsonData, 5*time.Minute).Err(); err != nil {
				log.Printf("Warning: failed to cache detail for %d: %v", id, err)
			}
		}
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

	// Invalidate the detail cache if it exists (since a listing conceptually updates the asset)
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

	// Invalidate appropriate caches after transfer
	if h.redisClient != nil {
		ctx := context.Background()
		detailKey := fmt.Sprintf("kor:asset:detail:%d", req.AssetID)
		if err := h.redisClient.Del(ctx, detailKey).Err(); err != nil {
			log.Printf("Warning: failed to invalidate cache for asset %d: %v", req.AssetID, err)
		}
	}

	c.JSON(http.StatusOK, transaction)
}
