package config

import (
	"fmt"
	"os"

	"github.com/yourusername/kor-assetforge/models"
	"github.com/yourusername/kor-assetforge/utils"
	"gorm.io/driver/postgres"
	"gorm.io/gorm"
)

// InitDB initializes the database connection
func InitDB() (*gorm.DB, error) {
	dsn := os.Getenv("DATABASE_URL")
	if dsn == "" {
		dsn = "host=localhost user=postgres password=password dbname=assetforge port=5432 sslmode=disable"
	}

	db, err := gorm.Open(postgres.Open(dsn), &gorm.Config{})
	if err != nil {
		return nil, fmt.Errorf("failed to connect to database: %w", err)
	}

	// Auto-migrate models
	if err := db.AutoMigrate(
		&models.Asset{},
		&models.Listing{},
		&models.Transaction{},
		&models.User{},
		&models.UserBalance{},
	); err != nil {
		return nil, fmt.Errorf("failed to migrate database: %w", err)
	}

	return db, nil
}

// InitStellarClient initializes the Stellar client
func InitStellarClient() (*utils.StellarClient, error) {
	horizonURL := os.Getenv("STELLAR_HORIZON_URL")
	networkType := os.Getenv("STELLAR_NETWORK")

	if networkType == "" {
		networkType = "testnet"
	}

	return utils.NewStellarClient(horizonURL, networkType)
}
