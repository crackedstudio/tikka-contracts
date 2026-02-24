#!/bin/bash
set -e

# Source environment variables if .env exists locally
if [ -f .env ]; then
  export $(cat .env | xargs)
fi

PUBLIC_KEY=$1

if [ -z "$PUBLIC_KEY" ]; then
    echo "Usage: ./scripts/fund-testnet.sh <stellar_public_key>"
    exit 1
fi

echo "Funding account $PUBLIC_KEY on Testnet..."
curl -s "https://friendbot.stellar.org/?addr=$PUBLIC_KEY"

echo ""
echo "Funding complete!"
