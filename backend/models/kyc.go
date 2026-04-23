package models

import (
	"time"

	"gorm.io/gorm"
)

// KYCStatus represents the verification lifecycle state.
type KYCStatus string

const (
	KYCStatusPending  KYCStatus = "pending"
	KYCStatusReview   KYCStatus = "review"
	KYCStatusApproved KYCStatus = "approved"
	KYCStatusRejected KYCStatus = "rejected"
	KYCStatusExpired  KYCStatus = "expired"
)

// AMLRiskLevel classifies the AML risk of a user.
type AMLRiskLevel string

const (
	AMLRiskLow    AMLRiskLevel = "low"
	AMLRiskMedium AMLRiskLevel = "medium"
	AMLRiskHigh   AMLRiskLevel = "high"
)

// KYCRecord stores identity-verification data for a platform user.
// Sensitive fields (DocumentNumber) are never serialised to JSON.
type KYCRecord struct {
	ID                 uint           `gorm:"primaryKey"             json:"id"`
	UserID             uint           `gorm:"not null;uniqueIndex"   json:"user_id"`
	User               User           `gorm:"foreignKey:UserID"      json:"user,omitempty"`
	ProviderRecordID   string         `gorm:"uniqueIndex"            json:"provider_record_id"`
	Status             KYCStatus      `gorm:"default:'pending'"      json:"status"`
	FullName           string         `gorm:"not null"               json:"full_name"`
	DateOfBirth        string         `                              json:"date_of_birth"`
	Nationality        string         `                              json:"nationality"`
	DocumentType       string         `                              json:"document_type"` // passport | driver_license | national_id
	DocumentNumberHash string         `gorm:"column:document_number_hash" json:"-"`         // SHA-256, never exposed
	RiskScore          int            `gorm:"default:0"              json:"risk_score"`
	AMLCleared         bool           `gorm:"default:false"          json:"aml_cleared"`
	AccreditedInvestor bool           `gorm:"default:false"          json:"accredited_investor"`
	ReviewNotes        string         `gorm:"type:text"              json:"review_notes,omitempty"`
	ExpiresAt          *time.Time     `                              json:"expires_at"`
	CreatedAt          time.Time      `                              json:"created_at"`
	UpdatedAt          time.Time      `                              json:"updated_at"`
	DeletedAt          gorm.DeletedAt `gorm:"index"                  json:"-"`
}

// KYCDocument tracks a document uploaded for identity verification.
type KYCDocument struct {
	ID          uint           `gorm:"primaryKey"    json:"id"`
	KYCRecordID uint           `gorm:"not null"      json:"kyc_record_id"`
	KYCRecord   KYCRecord      `gorm:"foreignKey:KYCRecordID" json:"-"`
	DocumentType string        `gorm:"not null"      json:"document_type"` // front | back | selfie | proof_of_address
	FileName    string         `gorm:"not null"      json:"file_name"`
	FileHash    string         `gorm:"not null"      json:"file_hash"` // SHA-256 of the uploaded bytes
	StoragePath string         `gorm:"not null"      json:"-"`         // internal path, never exposed
	MimeType    string         `                     json:"mime_type"`
	SizeBytes   int64          `                     json:"size_bytes"`
	Status      string         `gorm:"default:'pending'" json:"status"` // pending | accepted | rejected
	CreatedAt   time.Time      `                     json:"created_at"`
	DeletedAt   gorm.DeletedAt `gorm:"index"         json:"-"`
}

// AMLScreening records a single AML check against an external watchlist.
type AMLScreening struct {
	ID          uint           `gorm:"primaryKey"   json:"id"`
	KYCRecordID uint           `gorm:"not null"     json:"kyc_record_id"`
	ScreeningID string         `gorm:"uniqueIndex"  json:"screening_id"` // provider-assigned ID
	Status      string         `gorm:"default:'pending'" json:"status"` // pending | clear | flagged
	RiskLevel   AMLRiskLevel   `                    json:"risk_level"`
	Matches     string         `gorm:"type:text"   json:"matches"` // JSON array of match descriptions
	ScreenedAt  *time.Time     `                    json:"screened_at"`
	CreatedAt   time.Time      `                    json:"created_at"`
	DeletedAt   gorm.DeletedAt `gorm:"index"        json:"-"`
}

// ComplianceAuditLog is an immutable append-only record of every compliance action.
type ComplianceAuditLog struct {
	ID         uint      `gorm:"primaryKey" json:"id"`
	UserID     uint      `gorm:"not null"   json:"user_id"`
	Action     string    `gorm:"not null"   json:"action"`    // e.g. kyc_submitted, aml_screened, status_changed
	EntityType string    `                  json:"entity_type"` // KYCRecord | KYCDocument | AMLScreening
	EntityID   uint      `                  json:"entity_id"`
	Details    string    `gorm:"type:text"  json:"details"`   // JSON blob with before/after or extra context
	IPAddress  string    `                  json:"ip_address"`
	CreatedAt  time.Time `                  json:"created_at"`
}
