#!/bin/bash
set -e

# Source environment variables if .env exists locally
if [ -f .env ]; then
  export $(cat .env | xargs)
fi

NETWORK="${STELLAR_NETWORK:-testnet}"
CONTRACT_ID="${RAFFLE_CONTRACT_ADDRESS}"
WASM_FILE="target/wasm32v1-none/release/hello_world.wasm"

if [ -z "$CONTRACT_ID" ]; then
    echo "Error: RAFFLE_CONTRACT_ADDRESS environment variable is required"
    exit 1
fi

if [ ! -f "$WASM_FILE" ]; then
    echo "Error: Local WASM file not found at $WASM_FILE. Please build first."
    exit 1
fi

echo "Verifying contract $CONTRACT_ID on $NETWORK..."

# Fetch remote WASM hash using stellar CLI
# stellar contract inspect --id $CONTRACT_ID output usually contains the hash
# or you can use `stellar template fetch` to fetch the code 
REMOTE_INFO=$(stellar contract read --id "$CONTRACT_ID" --network "$NETWORK" 2>/dev/null || true)
# NOTE: The exact CLI command to get the remote WASM hash varies by stellar CLI version. 
# Another approach is restoring the contract WASM directly and computing hash.
stellar contract fetch --id "$CONTRACT_ID" --network "$NETWORK" --out-file remote.wasm

if [ ! -f "remote.wasm" ]; then
    echo "Error: Failed to fetch remote contract."
    exit 1
fi

# Compare hashes
if command -v sha256sum >/dev/null 2>&1; then
    LOCAL_HASH=$(sha256sum "$WASM_FILE" | awk '{ print $1 }')
    REMOTE_HASH=$(sha256sum remote.wasm | awk '{ print $1 }')
elif command -v shasum >/dev/null 2>&1; then
    LOCAL_HASH=$(shasum -a 256 "$WASM_FILE" | awk '{ print $1 }')
    REMOTE_HASH=$(shasum -a 256 remote.wasm | awk '{ print $1 }')
else
    echo "Error: Neither sha256sum nor shasum is available to verify hashes."
    rm remote.wasm
    exit 1
fi

echo "Local WASM Hash:  $LOCAL_HASH"
echo "Remote WASM Hash: $REMOTE_HASH"

# Cleanup
rm remote.wasm

if [ "$LOCAL_HASH" = "$REMOTE_HASH" ]; then
    echo "Verification Result: Match: YES"
    exit 0
else
    echo "Verification Result: Match: NO"
    exit 1
fi
