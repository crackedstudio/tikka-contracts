#!/bin/bash
set -e

# Source environment variables if .env exists locally
if [ -f .env ]; then
  export $(cat .env | xargs)
fi

# Configuration
NETWORK="${STELLAR_NETWORK:-testnet}"
CONTRACT_ID="${RAFFLE_CONTRACT_ADDRESS}"

if [ -z "$CONTRACT_ID" ]; then
    echo "Error: RAFFLE_CONTRACT_ADDRESS is required"
    exit 1
fi

if [ -z "$1" ]; then
    echo "Usage: ./scripts/invoke.sh <function_name> [args...]"
    echo "Example: ./scripts/invoke.sh buy_ticket --source \$DEPLOYER_SECRET_KEY"
    exit 1
fi

FUNCTION_NAME=$1
shift # Shift arguments so $@ contains only the remaining args

echo "Invoking $FUNCTION_NAME on contract $CONTRACT_ID ($NETWORK)..."

stellar contract invoke \
  --id "$CONTRACT_ID" \
  --network "$NETWORK" \
  --source "${DEPLOYER_SECRET_KEY}" \
  -- "$FUNCTION_NAME" "$@"
