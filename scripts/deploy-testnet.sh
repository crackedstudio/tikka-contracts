#!/usr/bin/env bash
# Deploy a fully initialised raffle factory to testnet.
#
# One command produces a factory that can create raffles: the instance WASM is
# installed, the factory is deployed and initialised, the result is verified
# against the chain, and the deployment is recorded with enough detail to
# identify the code running at the address (issues #842, #843).

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/common.sh
source "${SCRIPT_DIR}/common.sh"

NETWORK="testnet"

load_env
require_env DEPLOYER_SECRET_KEY "the account that signs the deployment"
require_env ADMIN_ADDRESS "factory admin, passed to init_factory"

TREASURY_ADDRESS="${TREASURY_ADDRESS:-${ADMIN_ADDRESS}}"
PROTOCOL_FEE_BP="${PROTOCOL_FEE_BP:-0}"

warn_on_cli_mismatch
require_no_existing_deployment "${NETWORK}"

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
