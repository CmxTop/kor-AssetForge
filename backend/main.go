package main

import (
	"context"
	"log"
	"os"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/joho/godotenv"
	"github.com/prometheus/client_golang/prometheus/promhttp"
	swaggerFiles "github.com/swaggo/files"
	ginSwagger "github.com/swaggo/gin-swagger"
	"github.com/ulule/limiter/v3"
	_ "github.com/yourusername/kor-assetforge/docs"
	"github.com/yourusername/kor-assetforge/config"
	"github.com/yourusername/kor-assetforge/handlers"
	"github.com/yourusername/kor-assetforge/middleware"
	"github.com/yourusername/kor-assetforge/services"
	"github.com/yourusername/kor-assetforge/utils"
	"github.com/yourusername/kor-assetforge/validator"
)

// @title kor-AssetForge API
// @version 0.1.0
// @description Decentralized marketplace for tokenizing and trading real-world assets on Stellar.
// @termsOfService http://swagger.io/terms/

// @contact.name API Support
// @contact.url http://www.swagger.io/support
// @contact.email support@swagger.io

// @license.name MIT
// @license.url https://opensource.org/licenses/MIT

// @host localhost:8080
// @BasePath /api/v1
func main() {
	// Load environment variables
	if err := godotenv.Load(); err != nil {
		log.Println("No .env file found, using system environment variables")
	}

	// Initialize database
	db, err := config.InitDB()
	if err != nil {
		log.Fatalf("Failed to connect to database: %v", err)
	}

	// Initialize Stellar client
	stellarClient, err := config.InitStellarClient()
	if err != nil {
		log.Fatalf("Failed to initialize Stellar client: %v", err)
	}

	// Initialize Redis
	redisURL := os.Getenv("REDIS_URL")
	redisClient, err := utils.InitRedis(redisURL)
	if err != nil {
		log.Printf("Warning: Failed to initialize Redis, continuing without cache: %v", err)
		redisClient = nil
	} else {
		defer redisClient.Close()
	}

	// Initialize advanced cache manager (wraps Redis with L1 + metrics)
	cacheManager := utils.NewCacheManager(redisClient)

	// Warm common cache entries on startup
	go cacheManager.Warm(context.Background(), config.WarmCacheEntries(db))
	// Initialize Rate Limiter (e.g., 100 requests per minute)
	var rateLimiterMiddleware gin.HandlerFunc
	if redisClient != nil {
		rl, err := handlers.NewRateLimiter(redisClient, limiter.Rate{
			Period: time.Minute,
			Limit:  100,
		})
		if err != nil {
			log.Printf("Warning: Failed to initialize rate limiter: %v", err)
		} else {
			rateLimiterMiddleware = rl.Middleware()
		}
	}

	// Setup router
	router := gin.New()

	if err := validator.Init(); err != nil {
		log.Fatalf("Failed to initialize validator: %v", err)
	}

	// Use custom enhanced middleware
	router.Use(
		handlers.RequestLogger(),
		handlers.GlobalErrorHandler(),
		middleware.RequestSizeLimiter(2<<20),
		middleware.RequireJSON(),
		middleware.RateLimit(20, time.Minute),
		middleware.CSRFProtection(os.Getenv("CSRF_SECRET")),
	)

	// Health check handlers
	healthHandler := handlers.NewHealthHandler(db, redisClient, stellarClient)
	router.GET("/health", healthHandler.LivenessCheck)
	router.GET("/health/ready", healthHandler.ReadinessCheck)
	router.GET("/health/live", healthHandler.LivenessCheck)

	// Metrics endpoint
	// @Summary Prometheus metrics
	// @Description Get service metrics in Prometheus format
	// @Tags monitoring
	// @Produce plain
	// @Success 200 {string} string "Prometheus metrics"
	// @Router /metrics [get]
	router.GET("/metrics", gin.WrapH(promhttp.Handler()))

	// Swagger documentation
	router.GET("/swagger/*any", ginSwagger.WrapHandler(swaggerFiles.Handler))

	// Cache metrics
	router.GET("/metrics/cache", middleware.CacheMetricsHandler(cacheManager))

	// API v1 routes
	v1 := router.Group("/api/v1")
	{
		// Asset routes (with write-through cache invalidation)
		// Asset routes
		assetHandler := handlers.NewAssetHandler(db, stellarClient, redisClient)
		v1.POST("/assets/tokenize",
			middleware.InvalidateOnWrite(cacheManager, "kor:asset:*"),
			assetHandler.TokenizeAsset)
		v1.POST("/assets",
			middleware.InvalidateOnWrite(cacheManager, "kor:asset:*"),
			assetHandler.TokenizeAsset)
		v1.GET("/assets",
			middleware.HTTPCache(cacheManager, 5*time.Minute, "kor:asset", nil),
			assetHandler.ListAssets)
		v1.GET("/assets/:id",
			middleware.HTTPCache(cacheManager, 5*time.Minute, "kor:asset", nil),
			assetHandler.GetAsset)

		// Marketplace routes
		v1.POST("/marketplace/list",
			middleware.InvalidateOnWrite(cacheManager, "kor:asset:*"),
			assetHandler.ListAssetForSale)
		v1.POST("/marketplace/transfer",
			middleware.InvalidateOnWrite(cacheManager, "kor:asset:*"),
			assetHandler.TransferAsset)
		v1.GET("/transactions", assetHandler.ListTransactions)

		// Search routes (#57)
		searchBackend := services.NewESSearchBackend(os.Getenv("ELASTICSEARCH_URL"), db)
		searchHandler := handlers.NewSearchHandler(searchBackend)
		v1.GET("/search/assets", searchHandler.Search)
		v1.GET("/search/suggestions", searchHandler.Suggest)
		v1.GET("/search/analytics", searchHandler.SearchAnalytics)

		// KYC / AML routes (#55)
		kycHandler := handlers.NewKYCHandler(db, nil) // nil = mock provider
		v1.POST("/kyc/submit", kycHandler.SubmitKYC)
		v1.GET("/kyc/status", kycHandler.GetKYCStatus)
		v1.POST("/kyc/documents", kycHandler.UploadDocument)
		v1.POST("/kyc/aml/screen", kycHandler.ScreenAML)
		v1.POST("/kyc/accredited", kycHandler.VerifyAccreditedInvestor)
		v1.GET("/kyc/audit", kycHandler.GetAuditLog)
		v1.GET("/compliance/report", kycHandler.ComplianceReport)

		// Webhook routes
		webhookHandler := handlers.NewWebhookHandler(db)
		router.POST("/webhooks/stellar-events", webhookHandler.HandleStellarEvent)
		router.POST("/webhooks/kyc", kycHandler.HandleKYCWebhook)
	}

	// WebSocket routes (#54) — outside v1 group so the CSRF/JSON middleware
	// does not block the Upgrade handshake.
	wsHandler := handlers.NewWebSocketHandler()
	router.GET("/ws", wsHandler.HandleWS)
	router.GET("/ws/stats", wsHandler.HandleWSStats)

	// Pre-launch the hub so it's ready before the first connection.
	_ = handlers.GetHub()

	// Start server
	port := os.Getenv("SERVER_PORT")
	if port == "" {
		port = "8080"
	}

	log.Printf("Starting server on port %s", port)
	if err := router.Run(":" + port); err != nil {
		log.Fatalf("Failed to start server: %v", err)
	}
}
