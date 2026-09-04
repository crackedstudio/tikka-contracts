#!/usr/bin/env bash
# Compare the bytecode deployed at a contract address against the local build.
#
# Exit 0 on a match, 1 on a mismatch or a fetch failure, so callers can gate a
# deployment on it (issue #843).

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/common.sh
source "${SCRIPT_DIR}/common.sh"

load_env

NETWORK="${STELLAR_NETWORK:-testnet}"
CONTRACT_ID="${RAFFLE_CONTRACT_ADDRESS:-}"

require_cmd stellar
require_env RAFFLE_CONTRACT_ADDRESS "the contract address to verify"

if [[ ! -f "${FACTORY_WASM}" ]]; then
    echo "Error: local artifact not found at ${FACTORY_WASM}. Build first." >&2
    exit 1
fi

REMOTE_WASM="$(mktemp -t remote-wasm.XXXXXX)"
trap 'rm -f "${REMOTE_WASM}"' EXIT

echo "Verifying contract ${CONTRACT_ID} on ${NETWORK}..."

stellar contract fetch --id "${CONTRACT_ID}" --network "${NETWORK}" --out-file "${REMOTE_WASM}"

if [[ ! -s "${REMOTE_WASM}" ]]; then
    echo "Error: failed to fetch remote contract." >&2
    exit 1
fi

LOCAL_HASH="$(sha256_of "${FACTORY_WASM}")"
REMOTE_HASH="$(sha256_of "${REMOTE_WASM}")"

echo "Local WASM Hash:  ${LOCAL_HASH}"
echo "Remote WASM Hash: ${REMOTE_HASH}"

if [[ "${LOCAL_HASH}" = "${REMOTE_HASH}" ]]; then
    echo "Verification Result: Match: YES"
    exit 0
fi

echo "Verification Result: Match: NO" >&2
echo "The deployed bytecode does not match the local build. Check that you are" >&2
echo "on the same commit and using the pinned toolchain (docs/DEPLOYMENT.md)." >&2
exit 1
