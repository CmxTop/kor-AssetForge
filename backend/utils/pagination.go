package utils

import (
	"fmt"
	"math"
	"strconv"

	"github.com/gin-gonic/gin"
	"gorm.io/gorm"
)

// Pagination represents the pagination parameters and metadata
type Pagination struct {
	Limit      int         `json:"limit"`
	Page       int         `json:"page"`
	Total      int64       `json:"total"`
	TotalPages int         `json:"total_pages"`
	HasNext    bool        `json:"has_next"`
	HasPrev    bool        `json:"has_prev"`
	Links      Links       `json:"links"`
	Data       interface{} `json:"data"`
}

// Links represents HATEOAS links
type Links struct {
	Self  string `json:"self"`
	First string `json:"first"`
	Prev  string `json:"prev,omitempty"`
	Next  string `json:"next,omitempty"`
	Last  string `json:"last"`
}

// GetPaginationParams extracts page and limit from query parameters
func GetPaginationParams(c *gin.Context) (int, int) {
	limitStr := c.DefaultQuery("limit", "10")
	pageStr := c.DefaultQuery("page", "1")

	limit, _ := strconv.Atoi(limitStr)
	if limit <= 0 || limit > 100 {
		limit = 10
	}

	page, _ := strconv.Atoi(pageStr)
	if page <= 0 {
		page = 1
	}

	return page, limit
}

// Paginate applies pagination to a GORM query and returns enhanced metadata
func Paginate(db *gorm.DB, c *gin.Context, page, limit int, total *int64, value interface{}) (Pagination, error) {
	// Get total count
	if err := db.Count(total).Error; err != nil {
		return Pagination{}, err
	}

	// Apply offset and limit
	offset := (page - 1) * limit
	if err := db.Offset(offset).Limit(limit).Find(value).Error; err != nil {
		return Pagination{}, err
	}

	totalPages := int(math.Ceil(float64(*total) / float64(limit)))
	if totalPages == 0 && *total > 0 {
		totalPages = 1
	}

	basePath := c.Request.URL.Path
	links := Links{
		Self:  fmt.Sprintf("%s?page=%d&limit=%d", basePath, page, limit),
		First: fmt.Sprintf("%s?page=1&limit=%d", basePath, limit),
		Last:  fmt.Sprintf("%s?page=%d&limit=%d", basePath, totalPages, limit),
	}

	if page > 1 {
		links.Prev = fmt.Sprintf("%s?page=%d&limit=%d", basePath, page-1, limit)
	}
	if page < totalPages {
		links.Next = fmt.Sprintf("%s?page=%d&limit=%d", basePath, page+1, limit)
	}

	// Support Link header
	linkHeader := fmt.Sprintf("<%s>; rel=\"self\", <%s>; rel=\"first\", <%s>; rel=\"last\"", links.Self, links.First, links.Last)
	if links.Prev != "" {
		linkHeader += fmt.Sprintf(", <%s>; rel=\"prev\"", links.Prev)
	}
	if links.Next != "" {
		linkHeader += fmt.Sprintf(", <%s>; rel=\"next\"", links.Next)
	}
	c.Header("Link", linkHeader)

	return Pagination{
		Limit:      limit,
		Page:       page,
		Total:      *total,
		TotalPages: totalPages,
		HasNext:    page < totalPages,
		HasPrev:    page > 1,
		Links:      links,
		Data:       value,
	}, nil
}
