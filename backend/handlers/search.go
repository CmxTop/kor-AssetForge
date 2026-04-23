package handlers

import (
	"net/http"
	"strconv"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/yourusername/kor-assetforge/services"
	"go.uber.org/zap"
)

// SearchHandler provides asset search and suggestion endpoints.
type SearchHandler struct {
	backend  services.SearchBackend
	analytics []services.SearchAnalyticsEvent // in-memory ring; replace with DB or queue in production
}

// NewSearchHandler constructs a SearchHandler. Pass a DBSearchBackend or
// ESSearchBackend (or any SearchBackend) as the backend.
func NewSearchHandler(backend services.SearchBackend) *SearchHandler {
	return &SearchHandler{backend: backend}
}

// Search handles GET /api/v1/search/assets
//
// Query params:
//
//	q          – free-text search term (name / symbol / description / asset_type)
//	asset_type – exact match filter
//	min_price  – minimum listing price_per_unit (stroops)
//	max_price  – maximum listing price_per_unit (stroops)
//	verified   – "true" or "false"
//	sort_by    – name | created_at | total_supply | fractions  (default: created_at)
//	order      – asc | desc  (default: desc)
//	page       – 1-based page number (default: 1)
//	limit      – page size 1-100 (default: 10)
func (sh *SearchHandler) Search(c *gin.Context) {
	var req services.SearchRequest
	if err := c.ShouldBindQuery(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	start := time.Now()
	result, err := sh.backend.Search(c.Request.Context(), &req)
	if err != nil {
		Logger.Error("asset search failed", zap.Error(err))
		c.JSON(http.StatusInternalServerError, gin.H{"error": "search unavailable"})
		return
	}

	tookMs := float64(time.Since(start).Microseconds()) / 1000.0
	result.Took = tookMs

	// Record analytics (fire-and-forget).
	sh.recordAnalytics(req, result.Total, tookMs)

	c.JSON(http.StatusOK, result)
}

// Suggest handles GET /api/v1/search/suggestions
//
// Query params:
//
//	q     – prefix to auto-complete (required)
//	limit – max suggestions returned (default: 10, max: 20)
func (sh *SearchHandler) Suggest(c *gin.Context) {
	query := c.Query("q")
	if query == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "q param required"})
		return
	}

	limit, _ := strconv.Atoi(c.DefaultQuery("limit", "10"))
	if limit <= 0 || limit > 20 {
		limit = 10
	}

	result, err := sh.backend.Suggest(c.Request.Context(), query, limit)
	if err != nil {
		Logger.Error("search suggest failed", zap.Error(err))
		c.JSON(http.StatusInternalServerError, gin.H{"error": "suggest unavailable"})
		return
	}

	c.JSON(http.StatusOK, result)
}

// SearchAnalytics handles GET /api/v1/search/analytics
// Returns the last N recorded search events for observability.
func (sh *SearchHandler) SearchAnalytics(c *gin.Context) {
	c.JSON(http.StatusOK, gin.H{
		"events": sh.analytics,
		"total":  len(sh.analytics),
	})
}

func (sh *SearchHandler) recordAnalytics(req services.SearchRequest, count int64, tookMs float64) {
	filters := map[string]interface{}{}
	if req.AssetType != "" {
		filters["asset_type"] = req.AssetType
	}
	if req.Verified != nil {
		filters["verified"] = *req.Verified
	}
	if req.MinPrice != nil {
		filters["min_price"] = *req.MinPrice
	}
	if req.MaxPrice != nil {
		filters["max_price"] = *req.MaxPrice
	}

	evt := services.SearchAnalyticsEvent{
		Query:       req.Query,
		Filters:     filters,
		ResultCount: count,
		TookMs:      tookMs,
	}

	// Keep the last 1000 events in memory.
	if len(sh.analytics) >= 1000 {
		sh.analytics = sh.analytics[1:]
	}
	sh.analytics = append(sh.analytics, evt)
}
