package middleware

import (
    "net/http"
    "net/http/httptest"
    "strings"
    "testing"
    "time"

    "github.com/gin-gonic/gin"
)

func TestRequireJSONRejectsUnsupportedMediaType(t *testing.T) {
    gin.SetMode(gin.TestMode)
    router := gin.New()
    router.Use(RequireJSON())
    router.POST("/test", func(c *gin.Context) {
        c.Status(http.StatusOK)
    })

    req := httptest.NewRequest(http.MethodPost, "/test", strings.NewReader("hello"))
    req.Header.Set("Content-Type", "text/plain")
    w := httptest.NewRecorder()
    router.ServeHTTP(w, req)

    if w.Code != http.StatusUnsupportedMediaType {
        t.Fatalf("expected %d status, got %d", http.StatusUnsupportedMediaType, w.Code)
    }
}

func TestCSRFProtectionRejectsMissingToken(t *testing.T) {
    gin.SetMode(gin.TestMode)
    router := gin.New()
    router.Use(CSRFProtection("secret-token"))
    router.POST("/test", func(c *gin.Context) {
        c.Status(http.StatusOK)
    })

    req := httptest.NewRequest(http.MethodPost, "/test", strings.NewReader("{}"))
    req.Header.Set("Content-Type", "application/json")
    w := httptest.NewRecorder()
    router.ServeHTTP(w, req)

    if w.Code != http.StatusForbidden {
        t.Fatalf("expected %d status, got %d", http.StatusForbidden, w.Code)
    }
}

func TestRateLimitBlocksExcessRequests(t *testing.T) {
    gin.SetMode(gin.TestMode)
    router := gin.New()
    router.Use(RateLimit(2, time.Second))
    router.GET("/test", func(c *gin.Context) {
        c.Status(http.StatusOK)
    })

    req := httptest.NewRequest(http.MethodGet, "/test", nil)
    req.RemoteAddr = "127.0.0.1:12345"

    for i := 0; i < 2; i++ {
        w := httptest.NewRecorder()
        router.ServeHTTP(w, req)
        if w.Code != http.StatusOK {
            t.Fatalf("expected OK on request %d, got %d", i+1, w.Code)
        }
    }

    w := httptest.NewRecorder()
    router.ServeHTTP(w, req)
    if w.Code != http.StatusTooManyRequests {
        t.Fatalf("expected %d status on rate limit exceed, got %d", http.StatusTooManyRequests, w.Code)
    }
}
