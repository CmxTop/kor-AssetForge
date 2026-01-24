#!/bin/bash

set -e

echo "🚀 Deploying kor-AssetForge contracts to Stellar Testnet..."

# Check if stellar CLI is installed
if ! command -v stellar &> /dev/null; then
    echo "❌ Stellar CLI not found. Run ./scripts/setup.sh first"
    exit 1
fi

# Build contracts
echo "Building contracts..."
cd contracts
cargo build --target wasm32-unknown-unknown --release
cd ..

# Set network to testnet
echo "Configuring Stellar network..."
stellar network add \
  --global testnet \
  --rpc-url https://soroban-testnet.stellar.org:443 \
  --network-passphrase "Test SDF Network ; September 2015"

# Generate identity if not exists
if ! stellar keys ls | grep -q "deployer"; then
    echo "Creating deployer identity..."
    stellar keys generate --global deployer --network testnet
fi

# Fund account
echo "Funding deployer account..."
stellar keys fund deployer --network testnet || true

# Deploy AssetToken contract
echo "Deploying AssetToken contract..."
ASSET_CONTRACT_ID=$(stellar contract deploy \
  --wasm contracts/target/wasm32-unknown-unknown/release/kor_assetforge_contracts.wasm \
  --source deployer \
  --network testnet)

echo "✅ AssetToken deployed: $ASSET_CONTRACT_ID"

# Deploy Marketplace contract
echo "Deploying Marketplace contract..."
MARKETPLACE_CONTRACT_ID=$(stellar contract deploy \
  --wasm contracts/target/wasm32-unknown-unknown/release/kor_assetforge_contracts.wasm \
  --source deployer \
  --network testnet)

echo "✅ Marketplace deployed: $MARKETPLACE_CONTRACT_ID"

# Save contract IDs
cat > backend/.contracts << EOF
ASSET_CONTRACT_ID=$ASSET_CONTRACT_ID
MARKETPLACE_CONTRACT_ID=$MARKETPLACE_CONTRACT_ID
EOF

echo ""
echo "✅ Deployment complete!"
echo ""
echo "Contract IDs saved to backend/.contracts"
echo "Add these to your .env file:"
echo "ASSET_CONTRACT_ID=$ASSET_CONTRACT_ID"
echo "MARKETPLACE_CONTRACT_ID=$MARKETPLACE_CONTRACT_ID"
