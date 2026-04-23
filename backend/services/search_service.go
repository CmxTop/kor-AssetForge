package services

import (
	"context"
	"fmt"
	"strings"

	"github.com/yourusername/kor-assetforge/models"
	"gorm.io/gorm"
)

// SearchRequest contains all filter / sort / pagination parameters.
type SearchRequest struct {
	Query      string  `form:"q"`
	AssetType  string  `form:"asset_type"`
	MinPrice   *int64  `form:"min_price"`
	MaxPrice   *int64  `form:"max_price"`
	Verified   *bool   `form:"verified"`
	SortBy     string  `form:"sort_by"`  // name | created_at | total_supply | fractions
	Order      string  `form:"order"`    // asc | desc
	Page       int     `form:"page"`
	Limit      int     `form:"limit"`
}

// SearchResult is the paginated response envelope.
type SearchResult struct {
	Total   int64          `json:"total"`
	Page    int            `json:"page"`
	Limit   int            `json:"limit"`
	Assets  []models.Asset `json:"assets"`
	Facets  SearchFacets   `json:"facets"`
	Took    float64        `json:"took_ms"` // query duration in milliseconds
}

// SearchFacets carries aggregated filter counts for faceted navigation.
type SearchFacets struct {
	AssetTypes []FacetBucket `json:"asset_types"`
	Verified   FacetBucket   `json:"verified"`
}

// FacetBucket is a single facet value with its document count.
type FacetBucket struct {
	Value string `json:"value"`
	Count int64  `json:"count"`
}

// SuggestResult holds lightweight search suggestions.
type SuggestResult struct {
	Suggestions []string `json:"suggestions"`
}

// SearchAnalyticsEvent is appended to the analytics log on every search.
type SearchAnalyticsEvent struct {
	Query      string         `json:"query"`
	Filters    map[string]interface{} `json:"filters"`
	ResultCount int64         `json:"result_count"`
	TookMs     float64        `json:"took_ms"`
}

// ---- SearchBackend interface -------------------------------------------------

// SearchBackend abstracts the underlying search engine.
// DBSearchBackend (PostgreSQL) is the default; ESSearchBackend is the mock
// Elasticsearch adapter that falls back to DB when ES is unavailable.
type SearchBackend interface {
	Search(ctx context.Context, req *SearchRequest) (*SearchResult, error)
	Suggest(ctx context.Context, query string, limit int) (*SuggestResult, error)
}

// ---- DBSearchBackend --------------------------------------------------------

// DBSearchBackend uses PostgreSQL ILIKE full-text search and GORM scopes.
// No external dependencies — works out of the box.
type DBSearchBackend struct {
	db *gorm.DB
}

// NewDBSearchBackend constructs a DBSearchBackend.
func NewDBSearchBackend(db *gorm.DB) SearchBackend { return &DBSearchBackend{db: db} }

func (s *DBSearchBackend) Search(ctx context.Context, req *SearchRequest) (*SearchResult, error) {
	q := s.db.WithContext(ctx).Model(&models.Asset{})

	// Full-text filter across name, symbol, description, asset_type
	if term := strings.TrimSpace(req.Query); term != "" {
		like := "%" + term + "%"
		q = q.Where(
			"name ILIKE ? OR symbol ILIKE ? OR description ILIKE ? OR asset_type ILIKE ?",
			like, like, like, like,
		)
	}

	// Exact asset-type filter
	if req.AssetType != "" {
		q = q.Where("asset_type = ?", req.AssetType)
	}

	// Verified flag
	if req.Verified != nil {
		q = q.Where("verified = ?", *req.Verified)
	}

	// Price range via JOIN with active listings
	if req.MinPrice != nil || req.MaxPrice != nil {
		q = q.Joins("JOIN listings ON listings.asset_id = assets.id AND listings.deleted_at IS NULL AND listings.active = true")
		if req.MinPrice != nil {
			q = q.Where("listings.price_per_unit >= ?", *req.MinPrice)
		}
		if req.MaxPrice != nil {
			q = q.Where("listings.price_per_unit <= ?", *req.MaxPrice)
		}
		q = q.Distinct("assets.*")
	}

	// Count before pagination
	var total int64
	if err := q.Count(&total).Error; err != nil {
		return nil, fmt.Errorf("search count: %w", err)
	}

	// Sort
	sortBy := req.SortBy
	if sortBy == "" {
		sortBy = "created_at"
	}
	allowedSort := map[string]bool{"name": true, "created_at": true, "total_supply": true, "fractions": true}
	if !allowedSort[sortBy] {
		sortBy = "created_at"
	}
	order := strings.ToLower(req.Order)
	if order != "asc" {
		order = "desc"
	}
	q = q.Order(sortBy + " " + order)

	// Pagination
	page, limit := req.Page, req.Limit
	if page < 1 {
		page = 1
	}
	if limit < 1 || limit > 100 {
		limit = 10
	}
	q = q.Offset((page - 1) * limit).Limit(limit)

	var assets []models.Asset
	if err := q.Find(&assets).Error; err != nil {
		return nil, fmt.Errorf("search query: %w", err)
	}

	facets := s.buildFacets(ctx, req)

	return &SearchResult{
		Total:  total,
		Page:   page,
		Limit:  limit,
		Assets: assets,
		Facets: facets,
	}, nil
}

// Suggest returns auto-complete suggestions based on asset names and symbols.
func (s *DBSearchBackend) Suggest(ctx context.Context, query string, limit int) (*SuggestResult, error) {
	if limit <= 0 || limit > 20 {
		limit = 10
	}
	like := strings.TrimSpace(query) + "%"
	if like == "%" {
		return &SuggestResult{Suggestions: []string{}}, nil
	}

	type row struct{ Name string }
	var names []row
	s.db.WithContext(ctx).Model(&models.Asset{}).
		Select("DISTINCT name").
		Where("name ILIKE ? OR symbol ILIKE ?", like, like).
		Limit(limit).Scan(&names)

	suggestions := make([]string, 0, len(names))
	for _, r := range names {
		suggestions = append(suggestions, r.Name)
	}
	return &SuggestResult{Suggestions: suggestions}, nil
}

func (s *DBSearchBackend) buildFacets(ctx context.Context, req *SearchRequest) SearchFacets {
	type typeBucket struct {
		AssetType string
		Count     int64
	}
	var buckets []typeBucket
	s.db.WithContext(ctx).Model(&models.Asset{}).
		Select("asset_type, COUNT(*) as count").
		Group("asset_type").
		Scan(&buckets)

	var verifiedCount, unverifiedCount int64
	s.db.WithContext(ctx).Model(&models.Asset{}).Where("verified = ?", true).Count(&verifiedCount)
	s.db.WithContext(ctx).Model(&models.Asset{}).Where("verified = ?", false).Count(&unverifiedCount)

	typeFacets := make([]FacetBucket, 0, len(buckets))
	for _, b := range buckets {
		typeFacets = append(typeFacets, FacetBucket{Value: b.AssetType, Count: b.Count})
	}

	_ = req // reserved for future range-aware facets
	return SearchFacets{
		AssetTypes: typeFacets,
		Verified:   FacetBucket{Value: "true", Count: verifiedCount},
	}
}

// ---- ESSearchBackend (mock Elasticsearch adapter) ---------------------------

// ESSearchBackend is a mock adapter that demonstrates how Elasticsearch would
// be integrated. When ES is unreachable it transparently falls back to the
// DBSearchBackend, so the API remains fully functional without a running ES
// cluster. In production, replace the fallback with a real ES client call.
type ESSearchBackend struct {
	baseURL string
	db      SearchBackend // fallback
}

// NewESSearchBackend creates an ESSearchBackend that delegates to db when ES
// is not configured or unavailable.
func NewESSearchBackend(esBaseURL string, db *gorm.DB) SearchBackend {
	return &ESSearchBackend{
		baseURL: esBaseURL,
		db:      NewDBSearchBackend(db),
	}
}

func (es *ESSearchBackend) Search(ctx context.Context, req *SearchRequest) (*SearchResult, error) {
	if es.baseURL == "" {
		// ES not configured → fall back to DB search transparently.
		return es.db.Search(ctx, req)
	}
	// TODO: call Elasticsearch _search endpoint here using es.baseURL.
	// For now, delegate to the DB backend (mock behaviour).
	return es.db.Search(ctx, req)
}

func (es *ESSearchBackend) Suggest(ctx context.Context, query string, limit int) (*SuggestResult, error) {
	if es.baseURL == "" {
		return es.db.Suggest(ctx, query, limit)
	}
	// TODO: call ES _suggest endpoint here.
	return es.db.Suggest(ctx, query, limit)
}
