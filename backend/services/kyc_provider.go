package services

import (
	"crypto/sha256"
	"fmt"
	"math/rand"
	"time"

	"github.com/google/uuid"
)

// VerificationResult is what any KYC provider returns after submitting a record.
type VerificationResult struct {
	ProviderRecordID string
	Status           string // pending | approved | rejected
	RiskScore        int    // 0-100; higher = riskier
	AMLCleared       bool
	ReviewNotes      string
}

// AMLResult is returned by an AML screening call.
type AMLResult struct {
	ScreeningID string
	Status      string       // pending | clear | flagged
	RiskLevel   string       // low | medium | high
	Matches     []string     // descriptions of watchlist hits
	ScreenedAt  time.Time
}

// KYCProvider is the interface every provider adapter must satisfy.
// The mock implementation below is used when no real provider is configured.
type KYCProvider interface {
	// SubmitVerification initiates identity verification for a user.
	SubmitVerification(userID uint, fullName, dob, nationality, docType, docNumberHash string) (*VerificationResult, error)

	// GetVerificationStatus polls for an update from the provider.
	GetVerificationStatus(providerRecordID string) (*VerificationResult, error)

	// ScreenAML runs anti-money-laundering checks against watchlists.
	ScreenAML(fullName, nationality string) (*AMLResult, error)

	// VerifyAccreditedInvestor checks whether net-worth evidence meets the threshold.
	VerifyAccreditedInvestor(userID uint, netWorthUSD float64) (bool, error)
}

// MockKYCProvider simulates a third-party KYC/AML service locally.
// All calls succeed immediately with deterministic mock data so that the full
// API surface is exercisable without a live external service.
type MockKYCProvider struct{}

// NewMockKYCProvider returns a ready-to-use MockKYCProvider.
func NewMockKYCProvider() KYCProvider { return &MockKYCProvider{} }

func (m *MockKYCProvider) SubmitVerification(
	userID uint, fullName, dob, nationality, docType, docNumberHash string,
) (*VerificationResult, error) {
	// Simulate a brief processing delay.
	time.Sleep(5 * time.Millisecond)

	return &VerificationResult{
		ProviderRecordID: uuid.NewString(),
		// New submissions always start as pending; a webhook would flip this.
		Status:      "pending",
		RiskScore:   0,
		AMLCleared:  false,
		ReviewNotes: "",
	}, nil
}

func (m *MockKYCProvider) GetVerificationStatus(providerRecordID string) (*VerificationResult, error) {
	if providerRecordID == "" {
		return nil, fmt.Errorf("providerRecordID is required")
	}
	// In the mock, 90 % of records come back approved.
	status := "approved"
	amlCleared := true
	riskScore := rand.Intn(20) //nolint:gosec
	notes := ""
	if rand.Intn(10) == 0 { //nolint:gosec
		status = "rejected"
		amlCleared = false
		riskScore = 70 + rand.Intn(30) //nolint:gosec
		notes = "Mock: document not legible"
	}
	return &VerificationResult{
		ProviderRecordID: providerRecordID,
		Status:           status,
		RiskScore:        riskScore,
		AMLCleared:       amlCleared,
		ReviewNotes:      notes,
	}, nil
}

func (m *MockKYCProvider) ScreenAML(fullName, nationality string) (*AMLResult, error) {
	if fullName == "" {
		return nil, fmt.Errorf("fullName is required for AML screening")
	}
	// Deterministic risk level based on name length (pure mock logic).
	riskLevel := "low"
	status := "clear"
	matches := []string{}
	if len(fullName) > 20 {
		riskLevel = "medium"
	}
	return &AMLResult{
		ScreeningID: uuid.NewString(),
		Status:      status,
		RiskLevel:   riskLevel,
		Matches:     matches,
		ScreenedAt:  time.Now(),
	}, nil
}

func (m *MockKYCProvider) VerifyAccreditedInvestor(userID uint, netWorthUSD float64) (bool, error) {
	// US SEC threshold: $1,000,000 net worth (excluding primary residence).
	const threshold = 1_000_000.0
	return netWorthUSD >= threshold, nil
}

// HashDocumentNumber returns the SHA-256 hex digest of a document number.
// The original number is never stored; only the hash is persisted.
func HashDocumentNumber(docNumber string) string {
	h := sha256.Sum256([]byte(docNumber))
	return fmt.Sprintf("%x", h)
}
