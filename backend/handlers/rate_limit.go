package handlers

import (
	"fmt"
	"net/http"
	"strconv"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/redis/go-redis/v9"
	"github.com/ulule/limiter/v3"
	sredis "github.com/ulule/limiter/v3/drivers/store/redis"
)

// RateLimiter holds the rate limiting logic
type RateLimiter struct {
	limiter *limiter.Limiter
}

// NewRateLimiter creates a new Redis-based rate limiter
func NewRateLimiter(client *redis.Client, rate limiter.Rate) (*RateLimiter, error) {
	store, err := sredis.NewStoreWithOptions(client, limiter.StoreOptions{
		Prefix: "rate_limiter:",
	})
	if err != nil {
		return nil, err
	}

	instance := limiter.New(store, rate)
	return &RateLimiter{limiter: instance}, nil
}

// Middleware returns a gin middleware for rate limiting
func (rl *RateLimiter) Middleware() gin.HandlerFunc {
	return func(c *gin.Context) {
		// Admin bypass
		if c.GetHeader("X-Admin-Bypass") == "true" {
			// In a real app, you'd verify a token or session here
			// For now, we'll check if the role is 'admin' if available in context
			if role, exists := c.Get("user_role"); exists && role == "admin" {
				c.Next()
				return
			}
		}

		// Key: Use user ID if available, otherwise IP
		key := c.ClientIP()
		if userID := c.GetHeader("X-User-ID"); userID != "" {
			key = userID
		}

		context, err := rl.limiter.Get(c, key)
		if err != nil {
			Logger.Error("Rate limiter error", fmt.Errorf("failed to get limit for key %s: %w", key, err))
			c.Next()
			return
		}

		// Set rate limit headers
		c.Header("X-RateLimit-Limit", strconv.FormatInt(context.Limit, 10))
		c.Header("X-RateLimit-Remaining", strconv.FormatInt(context.Remaining, 10))
		c.Header("X-RateLimit-Reset", strconv.FormatInt(context.Reset, 10))

		if context.Reached {
			c.JSON(http.StatusTooManyRequests, gin.H{
				"error":   "Too Many Requests",
				"message": "You have exceeded your rate limit. Please try again later.",
				"retry_after": time.Unix(context.Reset, 0).Sub(time.Now()).Seconds(),
			})
			c.Abort()
			return
		}

		c.Next()
	}
}
