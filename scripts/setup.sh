#!/bin/bash

set -e

echo "🚀 Setting up kor-AssetForge development environment..."

# Check prerequisites
echo "Checking prerequisites..."

if ! command -v cargo &> /dev/null; then
    echo "❌ Rust/Cargo not found. Please install from https://rustup.rs/"
    exit 1
fi

if ! command -v go &> /dev/null; then
    echo "❌ Go not found. Please install Go 1.21+"
    exit 1
fi

if ! command -v docker &> /dev/null; then
    echo "❌ Docker not found. Please install Docker"
    exit 1
fi

echo "✅ All prerequisites found"

# Install Stellar CLI
echo "Installing Stellar CLI..."
if ! command -v stellar &> /dev/null; then
    cargo install --locked stellar-cli --features opt
else
    echo "✅ Stellar CLI already installed"
fi

# Add wasm target for Rust
echo "Adding wasm32 target..."
rustup target add wasm32-unknown-unknown

# Setup contracts
echo "Setting up smart contracts..."
cd contracts
cargo build --target wasm32-unknown-unknown --release
cargo test
cd ..

# Setup backend
echo "Setting up backend..."
cd backend
go mod download
go mod tidy
cd ..

# Create .env file if it doesn't exist
if [ ! -f backend/.env ]; then
    echo "Creating .env file..."
    cat > backend/.env << EOF
DATABASE_URL=postgresql://postgres:password@localhost:5432/assetforge?sslmode=disable
STELLAR_NETWORK=testnet
STELLAR_HORIZON_URL=https://horizon-testnet.stellar.org
SERVER_PORT=8080
EOF
    echo "✅ Created backend/.env"
fi

echo ""
echo "✅ Setup complete!"
echo ""
echo "Next steps:"
echo "1. Start services: docker-compose up -d"
echo "2. Run backend: cd backend && go run main.go"
echo "3. Deploy contracts: ./scripts/deploy_contracts.sh"
