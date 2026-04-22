package middleware

import (
    "net/http"
    "strings"
    "sync"
    "time"

    "github.com/gin-gonic/gin"
    "golang.org/x/time/rate"
)

var (
    visitorLock sync.Mutex
    visitors    = make(map[string]*rate.Limiter)
)

func getVisitor(key string, r rate.Limit, burst int) *rate.Limiter {
    visitorLock.Lock()
    defer visitorLock.Unlock()

    limiter, exists := visitors[key]
    if !exists {
        limiter = rate.NewLimiter(r, burst)
        visitors[key] = limiter
    }

    return limiter
}

// RequestSizeLimiter limits the size of request bodies.
func RequestSizeLimiter(maxBytes int64) gin.HandlerFunc {
    return func(c *gin.Context) {
        if c.Request.ContentLength > maxBytes {
            c.AbortWithStatusJSON(http.StatusRequestEntityTooLarge, gin.H{
                "error": "Request body is too large",
            })
            return
        }
        c.Request.Body = http.MaxBytesReader(c.Writer, c.Request.Body, maxBytes)
        c.Next()
    }
}

// RequireJSON enforces JSON content types on state-changing requests.
func RequireJSON() gin.HandlerFunc {
    return func(c *gin.Context) {
        if c.Request.Method == http.MethodPost || c.Request.Method == http.MethodPut || c.Request.Method == http.MethodPatch {
            contentType := strings.ToLower(c.GetHeader("Content-Type"))
            if !strings.HasPrefix(contentType, "application/json") {
                c.AbortWithStatusJSON(http.StatusUnsupportedMediaType, gin.H{
                    "error": "Content-Type must be application/json",
                })
                return
            }
        }
        c.Next()
    }
}

// CSRFProtection validates a shared token for state changing requests.
func CSRFProtection(csrfSecret string) gin.HandlerFunc {
    return func(c *gin.Context) {
        if csrfSecret == "" {
            c.Next()
            return
        }

        if c.Request.Method == http.MethodPost || c.Request.Method == http.MethodPut || c.Request.Method == http.MethodPatch || c.Request.Method == http.MethodDelete {
            token := c.GetHeader("X-CSRF-Token")
            if token == "" || token != csrfSecret {
                c.AbortWithStatusJSON(http.StatusForbidden, gin.H{
                    "error": "Invalid CSRF token",
                })
                return
            }
        }
        c.Next()
    }
}

// RateLimit applies a simple per-client rate limit per endpoint.
func RateLimit(maxRequests int, window time.Duration) gin.HandlerFunc {
    if maxRequests <= 0 {
        maxRequests = 60
    }
    if window <= 0 {
        window = time.Minute
    }
    rateLimit := rate.Every(window / time.Duration(maxRequests))
    burst := maxRequests

    return func(c *gin.Context) {
        key := c.ClientIP() + "#" + c.FullPath()
        limiter := getVisitor(key, rateLimit, burst)
        if !limiter.Allow() {
            c.AbortWithStatusJSON(http.StatusTooManyRequests, gin.H{
                "error": "Rate limit exceeded",
            })
            return
        }
        c.Next()
    }
}
