#!/usr/bin/env bash
# Build both contracts with the pinned toolchain and print their SHA-256 hashes.
#
# This is the command a third party runs to check that the bytecode deployed at
# an address really is built from this source (issue #843). Compare its output
# against the hashes in deployments/<network>.json and on the GitHub release.
#
#     ./scripts/build-reproducible.sh
#     jq -r '.factoryWasmHash, .instanceWasmHash' deployments/testnet.json

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/common.sh
source "${SCRIPT_DIR}/common.sh"

require_cmd stellar
require_cmd rustup

CHANNEL="$(rust_toolchain_channel)"

echo "Toolchain"
echo "  rust:        ${CHANNEL} (rust-toolchain.toml)"
echo "  stellar-cli: ${STELLAR_CLI_VERSION} (pinned), $(stellar_cli_version) (installed)"
echo "  target:      ${WASM_TARGET}"
echo "  commit:      $(git -C "${REPO_ROOT}" rev-parse HEAD 2>/dev/null || echo unknown)"
echo ""

warn_on_cli_mismatch

# Start from a clean target dir so a stale artifact cannot be mistaken for a
# fresh build.
rm -rf "${WASM_DIR}"

build_contracts

echo ""
echo "SHA-256"
echo "  raffle_factory.wasm   $(sha256_of "${FACTORY_WASM}")"
echo "  raffle_instance.wasm  $(sha256_of "${INSTANCE_WASM}")"
