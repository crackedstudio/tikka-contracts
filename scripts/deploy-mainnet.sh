#!/bin/bash
set -e

# Source environment variables if .env exists locally
if [ -f .env ]; then
  export $(cat .env | xargs)
fi

echo "Building WASM..."
stellar contract build

WASM_FILE="target/wasm32v1-none/release/hello_world.wasm"

if [ ! -f "$WASM_FILE" ]; then
    echo "Error: WASM file not found at $WASM_FILE"
    exit 1
fi


echo "Deploying to Mainnet..."
# Requires DEPLOYER_SECRET_KEY to be set
if [ -z "$DEPLOYER_SECRET_KEY" ]; then
    echo "Error: DEPLOYER_SECRET_KEY is required to deploy"
    exit 1
fi

echo "WARNING: You are deploying to MAINNET. Proceed? (y/N)"
read -r response
if [[ ! "$response" =~ ^([yY][eE][sS]|[yY])+$ ]]
then
    echo "Deployment aborted."
    exit 1
fi

CONTRACT_ID=$(stellar contract deploy \
  --wasm "$WASM_FILE" \
  --source "${DEPLOYER_SECRET_KEY}" \
  --network mainnet)

echo "Deployment successful!"
echo "Contract ID: $CONTRACT_ID"

# Optionally write to deployments/mainnet.json
mkdir -p deployments
cat <<EOF > deployments/mainnet.json
{
  "network": "mainnet",
  "contractId": "$CONTRACT_ID",
  "timestamp": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
}
EOF

echo "Saved deployment info to deployments/mainnet.json"
