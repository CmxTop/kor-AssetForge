package models

import (
	"time"

	"gorm.io/gorm"
)

// User represents a platform user
type User struct {
	ID              uint           `gorm:"primaryKey" json:"id"`
	StellarAddress  string         `gorm:"uniqueIndex;not null" json:"stellar_address"`
	Email           string         `gorm:"uniqueIndex" json:"email"`
	Username        string         `gorm:"uniqueIndex" json:"username"`
	Role            string         `gorm:"default:'user'" json:"role"` // 'user' or 'admin'
	KYCVerified     bool           `gorm:"default:false" json:"kyc_verified"`
	AccreditedInvestor bool        `gorm:"default:false" json:"accredited_investor"`
	CreatedAt       time.Time      `json:"created_at"`
	UpdatedAt       time.Time      `json:"updated_at"`
	DeletedAt       gorm.DeletedAt `gorm:"index" json:"-"`
}

// UserBalance represents a user's token balance
type UserBalance struct {
	ID             uint      `gorm:"primaryKey" json:"id"`
	UserID         uint      `gorm:"not null" json:"user_id"`
	User           User      `gorm:"foreignKey:UserID" json:"user,omitempty"`
	AssetID        uint      `gorm:"not null" json:"asset_id"`
	Asset          Asset     `gorm:"foreignKey:AssetID" json:"asset,omitempty"`
	Balance        int64     `gorm:"not null;default:0" json:"balance"`
	LockedBalance  int64     `gorm:"not null;default:0" json:"locked_balance"` // For active listings
	UpdatedAt      time.Time `json:"updated_at"`
}
