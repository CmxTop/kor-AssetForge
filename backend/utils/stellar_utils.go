package utils

import (
	"fmt"

	"github.com/stellar/go/clients/horizonclient"
	"github.com/stellar/go/keypair"
	"github.com/stellar/go/network"
)

type StellarClient struct {
	HorizonClient *horizonclient.Client
	NetworkPass   string
}

// NewStellarClient creates a new Stellar client
func NewStellarClient(horizonURL string, networkType string) (*StellarClient, error) {
	client := horizonclient.DefaultTestNetClient
	networkPass := network.TestNetworkPassphrase

	if horizonURL != "" {
		client = &horizonclient.Client{
			HorizonURL: horizonURL,
		}
	}

	if networkType == "public" {
		networkPass = network.PublicNetworkPassphrase
	}

	return &StellarClient{
		HorizonClient: client,
		NetworkPass:   networkPass,
	}, nil
}

// ValidateAddress checks if a Stellar address is valid
func (sc *StellarClient) ValidateAddress(address string) error {
	_, err := keypair.ParseAddress(address)
	return err
}

// GetAccountBalance retrieves account balance
func (sc *StellarClient) GetAccountBalance(address string) (string, error) {
	account, err := sc.HorizonClient.AccountDetail(horizonclient.AccountRequest{
		AccountID: address,
	})
	if err != nil {
		return "", fmt.Errorf("failed to get account: %w", err)
	}

	// Return native balance (XLM)
	for _, balance := range account.Balances {
		if balance.Asset.Type == "native" {
			return balance.Balance, nil
		}
	}

	return "0", nil
}

// InvokeContract invokes a Soroban smart contract
func (sc *StellarClient) InvokeContract(contractID string, method string, params []interface{}) (string, error) {
	// TODO: Implement Soroban contract invocation
	// This requires the Soroban RPC client
	return "", fmt.Errorf("not implemented")
}
