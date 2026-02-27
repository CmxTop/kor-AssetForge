package models

import (
	"time"

	"gorm.io/gorm"
)

// Asset represents a tokenized real-world asset
type Asset struct {
	ID            uint           `gorm:"primaryKey" json:"id"`
	Name          string         `gorm:"not null" json:"name"`
	Symbol        string         `gorm:"not null;uniqueIndex" json:"symbol"`
	Description   string         `json:"description"`
	AssetType     string         `gorm:"not null" json:"asset_type"` // real_estate, art, commodity, etc.
	TotalSupply   int64          `gorm:"not null" json:"total_supply"`
	Fractions     uint64         `gorm:"default:0" json:"fractions"`
	ContractID    string         `gorm:"uniqueIndex" json:"contract_id"`
	OwnerAddress  string         `gorm:"not null" json:"owner_address"`
	Metadata      string         `gorm:"type:text" json:"metadata"` // JSON string of map[string]string
	ImageURL      string         `json:"image_url"`
	DocumentURL   string         `json:"document_url"`
	Verified      bool           `gorm:"default:false" json:"verified"`
	CreatedAt     time.Time      `json:"created_at"`
	UpdatedAt     time.Time      `json:"updated_at"`
	DeletedAt     gorm.DeletedAt `gorm:"index" json:"-"`
}

// Listing represents a marketplace listing
type Listing struct {
	ID          uint           `gorm:"primaryKey" json:"id"`
	AssetID     uint           `gorm:"not null" json:"asset_id"`
	Asset       Asset          `gorm:"foreignKey:AssetID" json:"asset,omitempty"`
	SellerAddr  string         `gorm:"not null" json:"seller_address"`
	Amount      int64          `gorm:"not null" json:"amount"`
	PricePerUnit int64         `gorm:"not null" json:"price_per_unit"` // in stroops
	Active      bool           `gorm:"default:true" json:"active"`
	ListingID   string         `gorm:"uniqueIndex" json:"listing_id"` // On-chain listing ID
	CreatedAt   time.Time      `json:"created_at"`
	UpdatedAt   time.Time      `json:"updated_at"`
	DeletedAt   gorm.DeletedAt `gorm:"index" json:"-"`
}

// Transaction represents an asset transfer
type Transaction struct {
	ID          uint      `gorm:"primaryKey" json:"id"`
	AssetID     uint      `gorm:"not null" json:"asset_id"`
	Asset       Asset     `gorm:"foreignKey:AssetID" json:"asset,omitempty"`
	FromAddress string    `gorm:"not null" json:"from_address"`
	ToAddress   string    `gorm:"not null" json:"to_address"`
	Amount      int64     `gorm:"not null" json:"amount"`
	TxHash      string    `gorm:"uniqueIndex" json:"tx_hash"`
	Status      string    `gorm:"default:'pending'" json:"status"` // pending, confirmed, failed
	CreatedAt   time.Time `json:"created_at"`
}
