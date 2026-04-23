package main

import (
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
	"github.com/yourusername/kor-assetforge/validator"
	"github.com/yourusername/kor-assetforge/utils"
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
	router := gin.New() // Use gin.New() instead of gin.Default() to avoid default logger/recovery

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

	// API v1 routes
	v1 := router.Group("/api/v1")
	{
		// Asset routes
		assetHandler := handlers.NewAssetHandler(db, stellarClient, redisClient)
		v1.POST("/assets/tokenize", assetHandler.TokenizeAsset)
		v1.POST("/assets", assetHandler.TokenizeAsset) 
		v1.GET("/assets", assetHandler.ListAssets)
		v1.GET("/assets/:id", assetHandler.GetAsset)

		// Marketplace routes
		v1.POST("/marketplace/list", assetHandler.ListAssetForSale)
		v1.POST("/marketplace/transfer", assetHandler.TransferAsset)
		v1.GET("/transactions", assetHandler.ListTransactions)

		// Webhook routes
		webhookHandler := handlers.NewWebhookHandler(db)
		router.POST("/webhooks/stellar-events", webhookHandler.HandleStellarEvent)
	}

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
