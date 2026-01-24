# kor-AssetForge

A decentralized marketplace for tokenizing and trading real-world assets (RWAs) on the Stellar network using Soroban smart contracts.

## Overview

kor-AssetForge enables fractional ownership of real-world assets like real estate, art, and commodities through blockchain tokenization. Built on Stellar's Soroban platform for efficient, low-cost transactions.

## Tech Stack

- **Smart Contracts**: Rust + Soroban SDK (Stellar)
- **Backend**: Go 1.21+ with Gin framework
- **Database**: PostgreSQL with GORM
- **Blockchain**: Stellar Testnet/Futurenet
- **Containerization**: Docker & Docker Compose

## Features

- Asset tokenization with fractional ownership
- On-chain marketplace for listing and trading
- RESTful API for frontend integration
- PostgreSQL for off-chain metadata storage
- Soroban smart contracts for trustless transactions

## Quick Start

### Prerequisites

- Rust 1.70+ and Cargo
- Go 1.21+
- Docker and Docker Compose
- Stellar CLI (soroban-cli)
- Git

### Setup

```bash
# Clone the repository
git clone https://github.com/yourusername/kor-AssetForge.git
cd kor-AssetForge

# Run setup script
chmod +x scripts/setup.sh
./scripts/setup.sh

# Start local development environment
docker-compose up -d

# Build and test smart contracts
cd contracts
cargo build --target wasm32-unknown-unknown --release
cargo test

# Run backend server
cd ../backend
go mod download
go run main.go
```

### Deploy to Stellar Testnet

```bash
chmod +x scripts/deploy_contracts.sh
./scripts/deploy_contracts.sh
```

## Project Structure

```
kor-AssetForge/
├── contracts/          # Soroban smart contracts (Rust)
├── backend/           # Go API server
├── docs/              # Documentation
├── scripts/           # Setup and deployment scripts
└── docker-compose.yml # Local development environment
```

## API Endpoints

- `GET /health` - Health check
- `POST /api/v1/assets` - Create new asset token
- `GET /api/v1/assets` - List all assets
- `GET /api/v1/assets/:id` - Get asset details
- `POST /api/v1/marketplace/list` - List asset for sale
- `POST /api/v1/marketplace/transfer` - Transfer asset ownership

## Environment Variables

Create a `.env` file in the backend directory:

```
DATABASE_URL=postgresql://postgres:password@localhost:5432/assetforge
STELLAR_NETWORK=testnet
STELLAR_HORIZON_URL=https://horizon-testnet.stellar.org
CONTRACT_ID=<your_deployed_contract_id>
SERVER_PORT=8080
```

## Testing

```bash
# Test smart contracts
cd contracts
cargo test

# Test backend
cd backend
go test ./...
```

## Security Best Practices

- Never commit private keys or secrets
- Use environment variables for sensitive data
- Validate all user inputs
- Implement rate limiting on APIs
- Use HTTPS in production
- Regular security audits for smart contracts

## Contributing

Contributions are welcome! Please open an issue or submit a pull request.

## License

MIT License - see [LICENSE](LICENSE) file for details

## Resources

- [Stellar Documentation](https://developers.stellar.org/)
- [Soroban Documentation](https://soroban.stellar.org/docs)
- [Stellar Go SDK](https://github.com/stellar/go)

## Support

For questions or support, please open an issue on GitHub.
