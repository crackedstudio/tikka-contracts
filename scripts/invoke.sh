#!/usr/bin/env bash
# Thin wrapper around `stellar contract invoke` for RAFFLE_CONTRACT_ADDRESS.

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/common.sh
source "${SCRIPT_DIR}/common.sh"

cd "${REPO_ROOT}"
load_env

NETWORK="${STELLAR_NETWORK:-testnet}"
CONTRACT_ID="${RAFFLE_CONTRACT_ADDRESS:-}"

require_cmd stellar
require_env RAFFLE_CONTRACT_ADDRESS "the contract to invoke"

if [[ -z "${1:-}" ]]; then
    usage "./scripts/invoke.sh <function_name> [args...]"
    echo "Example: ./scripts/invoke.sh get_admin" >&2
    exit 1
fi

FUNCTION_NAME="$1"
shift

echo "Invoking ${FUNCTION_NAME} on contract ${CONTRACT_ID} (${NETWORK})..."

stellar contract invoke \
  --id "${CONTRACT_ID}" \
  --network "${NETWORK}" \
  --source "${DEPLOYER_SECRET_KEY:-}" \
  -- "${FUNCTION_NAME}" "$@"
