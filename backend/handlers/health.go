package handlers

import (
	"context"
	"net/http"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/redis/go-redis/v9"
	"github.com/yourusername/kor-assetforge/utils"
	"gorm.io/gorm"
)

// HealthHandler handles health check requests
type HealthHandler struct {
	db            *gorm.DB
	redisClient   *redis.Client
	stellarClient *utils.StellarClient
}

// NewHealthHandler creates a new health handler
func NewHealthHandler(db *gorm.DB, redisClient *redis.Client, stellarClient *utils.StellarClient) *HealthHandler {
	return &HealthHandler{
		db:            db,
		redisClient:   redisClient,
		stellarClient: stellarClient,
	}
}

// LivenessCheck handles liveness probes
// @Summary Liveness check
// @Description Basic check to see if the service is up
// @Tags health
// @Produce json
// @Success 200 {object} map[string]interface{}
// @Router /health [get]
// @Router /health/live [get]
func (h *HealthHandler) LivenessCheck(c *gin.Context) {
	c.JSON(http.StatusOK, gin.H{"status": "UP"})
}

// ReadinessCheck handles readiness probes
// @Summary Readiness check
// @Description Comprehensive check to see if all dependencies are ready
// @Tags health
// @Produce json
// @Success 200 {object} map[string]interface{}
// @Failure 503 {object} map[string]interface{}
// @Router /health/ready [get]
func (h *HealthHandler) ReadinessCheck(c *gin.Context) {
	checks := make(map[string]string)
	isReady := true

	// Check Database
	sqlDB, err := h.db.DB()
	if err != nil {
		checks["database"] = "DOWN: " + err.Error()
		isReady = false
	} else if err := sqlDB.Ping(); err != nil {
		checks["database"] = "DOWN: " + err.Error()
		isReady = false
	} else {
		checks["database"] = "UP"
	}

	// Check Redis
	if h.redisClient != nil {
		ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
		defer cancel()
		if err := h.redisClient.Ping(ctx).Err(); err != nil {
			checks["redis"] = "DOWN: " + err.Error()
			isReady = false
		} else {
			checks["redis"] = "UP"
		}
	} else {
		checks["redis"] = "WARNING: Not Configured"
	}

	// Check Stellar Horizon
	if h.stellarClient != nil && h.stellarClient.HorizonClient != nil {
		_, err := h.stellarClient.HorizonClient.Root()
		if err != nil {
			checks["stellar_horizon"] = "DOWN: " + err.Error()
			isReady = false // Horizon being down might or might not mean we are not ready
		} else {
			checks["stellar_horizon"] = "UP"
		}
	} else {
		checks["stellar_horizon"] = "WARNING: Not Configured"
	}

	status := http.StatusOK
	if !isReady {
		status = http.StatusServiceUnavailable
	}

	c.JSON(status, gin.H{
		"status":      map[bool]string{true: "UP", false: "DOWN"}[isReady],
		"checks":      checks,
		"timestamp":   time.Now().Format(time.RFC3339),
	})
}
