package config

import (
	"fmt"
	"os"
	"time"

	"github.com/yourusername/kor-assetforge/models"
	"github.com/yourusername/kor-assetforge/utils"
	"gorm.io/driver/postgres"
	"gorm.io/gorm"
)

// InitDB initializes the database connection and auto-migrates all models.
func InitDB() (*gorm.DB, error) {
	dsn := os.Getenv("DATABASE_URL")
	if dsn == "" {
		dsn = "host=localhost user=postgres password=password dbname=assetforge port=5432 sslmode=disable"
	}

	db, err := gorm.Open(postgres.Open(dsn), &gorm.Config{})
	if err != nil {
		return nil, fmt.Errorf("failed to connect to database: %w", err)
	}

	if err := db.AutoMigrate(
		&models.Asset{},
		&models.Listing{},
		&models.Transaction{},
		&models.User{},
		&models.UserBalance{},
		// KYC / AML models (#55)
		&models.KYCRecord{},
		&models.KYCDocument{},
		&models.AMLScreening{},
		&models.ComplianceAuditLog{},
	); err != nil {
		return nil, fmt.Errorf("failed to migrate database: %w", err)
	}

	return db, nil
}

// InitStellarClient initializes the Stellar client.
func InitStellarClient() (*utils.StellarClient, error) {
	horizonURL := os.Getenv("STELLAR_HORIZON_URL")
	networkType := os.Getenv("STELLAR_NETWORK")
	if networkType == "" {
		networkType = "testnet"
	}
	return utils.NewStellarClient(horizonURL, networkType)
}

// WarmCacheEntries returns the list of keys to pre-populate on startup (#56).
// Loaders hit the database and the results are stored in the cache manager.
func WarmCacheEntries(db *gorm.DB) []utils.WarmEntry {
	return []utils.WarmEntry{
		{
			Key: "kor:asset:list:page1",
			TTL: 5 * time.Minute,
			Loader: func() (interface{}, error) {
				var assets []models.Asset
				if err := db.Order("created_at desc").Limit(10).Find(&assets).Error; err != nil {
					return nil, err
				}
				var total int64
				db.Model(&models.Asset{}).Count(&total)
				return map[string]interface{}{
					"limit": 10,
					"page":  1,
					"total": total,
					"data":  assets,
				}, nil
			},
		},
	}
}
