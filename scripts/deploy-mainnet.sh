#!/usr/bin/env bash
# Deploy a fully initialised raffle factory to mainnet.
#
# Identical to deploy-testnet.sh apart from the network and an explicit
# confirmation prompt (issues #842, #843).

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/common.sh
source "${SCRIPT_DIR}/common.sh"

NETWORK="mainnet"

load_env
require_env DEPLOYER_SECRET_KEY "the account that signs the deployment"
require_env ADMIN_ADDRESS "factory admin, passed to init_factory"

TREASURY_ADDRESS="${TREASURY_ADDRESS:-${ADMIN_ADDRESS}}"
PROTOCOL_FEE_BP="${PROTOCOL_FEE_BP:-0}"

warn_on_cli_mismatch
require_no_existing_deployment "${NETWORK}"

echo "WARNING: You are deploying to MAINNET."
echo "  admin:           ${ADMIN_ADDRESS}"
echo "  treasury:        ${TREASURY_ADDRESS}"
echo "  protocol fee bp: ${PROTOCOL_FEE_BP}"
echo "Proceed? (y/N)"
read -r response
if [[ ! "${response}" =~ ^([yY][eE][sS]|[yY])$ ]]; then
    echo "Deployment aborted."
    exit 1
fi

build_contracts

deploy_and_init_factory \
  "${NETWORK}" \
  "${DEPLOYER_SECRET_KEY}" \
  "${ADMIN_ADDRESS}" \
  "${TREASURY_ADDRESS}" \
  "${PROTOCOL_FEE_BP}"

verify_deployed_bytecode "${NETWORK}" "${FACTORY_CONTRACT_ID}"

write_deployment_manifest \
  "${NETWORK}" \
  "${ADMIN_ADDRESS}" \
  "${TREASURY_ADDRESS}" \
  "${PROTOCOL_FEE_BP}"

echo ""
echo "Deployment successful."
echo "Factory contract ID: ${FACTORY_CONTRACT_ID}"
echo "Instance WASM hash:  ${INSTANCE_WASM_HASH}"
