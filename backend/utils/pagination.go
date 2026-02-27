package utils

import (
	"strconv"

	"github.com/gin-gonic/gin"
	"gorm.io/gorm"
)

// Pagination represents the pagination parameters and metadata
type Pagination struct {
	Limit int         `json:"limit"`
	Page  int         `json:"page"`
	Total int64       `json:"total"`
	Data  interface{} `json:"data"`
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

// Paginate applies pagination to a GORM query
func Paginate(db *gorm.DB, page, limit int, total *int64, value interface{}) error {
	// Get total count
	if err := db.Count(total).Error; err != nil {
		return err
	}

	// Apply offset and limit
	offset := (page - 1) * limit
	return db.Offset(offset).Limit(limit).Find(value).Error
}
