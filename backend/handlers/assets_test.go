package handlers

import (
	"testing"

	"github.com/yourusername/kor-assetforge/validator"
)

func TestTokenizeAssetRequestValidation(t *testing.T) {
	if err := validator.Init(); err != nil {
		t.Fatalf("failed to initialize validator: %v", err)
	}

	req := validator.TokenizeAssetRequest{
		IssuerAccount: "GD6WU5I6OIPRZ4A5I3G6JQ4RG5K27SQ26WPQ5W3MXV6QABBT3C7FIEIF",
		Name:          "Real Asset",
		Symbol:        "RWA1",
		AssetType:     "real_estate",
		TotalSupply:   1000,
	}

	if err := validator.ValidateStruct(&req); err != nil {
		t.Fatalf("expected valid request to pass validation: %v", err)
	}
}

func TestTokenizeAssetRequestRejectsInvalidIssuer(t *testing.T) {
	if err := validator.Init(); err != nil {
		t.Fatalf("failed to initialize validator: %v", err)
	}

	req := validator.TokenizeAssetRequest{
		IssuerAccount: "INVALIDADDRESS",
		Name:          "Real Asset",
		Symbol:        "RWA1",
		AssetType:     "real_estate",
		TotalSupply:   1000,
	}

	if err := validator.ValidateStruct(&req); err == nil {
		t.Fatal("expected invalid issuer address to fail validation")
	}
}
