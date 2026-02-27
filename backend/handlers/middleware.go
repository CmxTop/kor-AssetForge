package handlers

import (
	"fmt"
	"net/http"
	"runtime/debug"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/google/uuid"
	"go.uber.org/zap"
)

var Logger *zap.Logger

func init() {
	var err error
	config := zap.NewProductionConfig()
	config.OutputPaths = []string{"stdout"}
	Logger, err = config.Build()
	if err != nil {
		panic(err)
	}
}

// RequestLogger middleware adds a request ID and logs detailed request/response info
func RequestLogger() gin.HandlerFunc {
	return func(c *gin.Context) {
		start := time.Now()
		requestID := uuid.New().String()
		c.Set("request_id", requestID)
		c.Header("X-Request-ID", requestID)

		// Process request
		c.Next()

		// Log request details
		duration := time.Since(start)
		Logger.Info("HTTP Request",
			zap.String("request_id", requestID),
			zap.String("method", c.Request.Method),
			zap.String("path", c.Request.URL.Path),
			zap.Int("status", c.Writer.Status()),
			zap.String("ip", c.ClientIP()),
			zap.Duration("duration", duration),
			zap.String("user_agent", c.Request.UserAgent()),
		)
	}
}

// GlobalErrorHandler middleware recovers from panics and standardizes error responses
func GlobalErrorHandler() gin.HandlerFunc {
	return func(c *gin.Context) {
		defer func() {
			if err := recover(); err != nil {
				// Log the panic with stack trace
				requestID, _ := c.Get("request_id")
				Logger.Error("Panic recovered",
					zap.Any("error", err),
					zap.Any("request_id", requestID),
					zap.String("stack", string(debug.Stack())),
				)

				// Standardized error response
				c.JSON(http.StatusInternalServerError, gin.H{
					"error":      "Internal Server Error",
					"message":    fmt.Sprintf("%v", err),
					"request_id": requestID,
					"code":       500,
				})
				c.Abort()
			}
		}()

		c.Next()

		// Check if there are errors in the context
		if len(c.Errors) > 0 {
			requestID, _ := c.Get("request_id")
			err := c.Errors.Last()
			
			// Standardized JSON error response
			c.JSON(c.Writer.Status(), gin.H{
				"error":      "Processing Error",
				"message":    err.Error(),
				"request_id": requestID,
				"code":       c.Writer.Status(),
			})
		}
	}
}
