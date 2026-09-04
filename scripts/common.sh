#!/usr/bin/env bash
# Shared definitions and helpers for every script under scripts/.
#
# This file is the single source of truth for the WASM target, the artifact
# paths and the pinned toolchain versions (issue #841), and for the deployment
# sequence shared by the testnet and mainnet scripts (issue #842).
#
# Source it; do not execute it:
#
#     source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"

# ── Build target and artifacts ───────────────────────────────────────────────
#
# `stellar contract build` targets wasm32v1-none. Everything that builds,
# size-checks, deploys or verifies a contract must agree on this, so the target
# and the artifact paths are derived here once and nowhere else (issue #841).
WASM_TARGET="wasm32v1-none"
WASM_DIR="${REPO_ROOT}/target/${WASM_TARGET}/release"
FACTORY_WASM="${WASM_DIR}/raffle_factory.wasm"
INSTANCE_WASM="${WASM_DIR}/raffle_instance.wasm"

# Maximum size of a single contract artifact, in bytes.
MAX_WASM_SIZE=131072

# ── Pinned toolchain ─────────────────────────────────────────────────────────
#
# The Rust compiler is pinned in rust-toolchain.toml; the Stellar CLI is pinned
# here. Both are recorded in every deployment manifest so a third party can
# reproduce the exact bytes that were deployed (issue #843).
STELLAR_CLI_VERSION="23.4.1"

rust_toolchain_channel() {
  awk -F'"' '/^channel[[:space:]]*=/ { print $2; exit }' "${REPO_ROOT}/rust-toolchain.toml"
}

# ── Environment ──────────────────────────────────────────────────────────────

load_env() {
  if [[ -f "${REPO_ROOT}/.env" ]]; then
    set -a
    # shellcheck disable=SC1091
    source "${REPO_ROOT}/.env"
    set +a
  fi
}

require_env() {
  local name="$1"
  local detail="${2:-}"
  if [[ -z "${!name:-}" ]]; then
    echo "Error: ${name} is required${detail:+ (${detail})}" >&2
    exit 1
  fi
}

require_cmd() {
  local name="$1"
  if ! command -v "${name}" >/dev/null 2>&1; then
    echo "Error: ${name} is required but was not found on PATH" >&2
    exit 1
  fi
}

usage() {
  echo "Usage: $1" >&2
}

# ── Toolchain checks ─────────────────────────────────────────────────────────

# Warn when the local Stellar CLI is not the pinned version. A mismatch does not
# stop a deployment, but it can change the produced bytes, so the manifest
# records whatever actually ran.
stellar_cli_version() {
  stellar --version 2>/dev/null | awk 'NR==1 { print $2 }'
}

warn_on_cli_mismatch() {
  local actual
  actual="$(stellar_cli_version)"
  if [[ "${actual}" != "${STELLAR_CLI_VERSION}" ]]; then
    echo "Warning: stellar CLI is ${actual:-unknown}, pinned version is ${STELLAR_CLI_VERSION}." >&2
    echo "         Builds may not be byte-reproducible. See docs/DEPLOYMENT.md." >&2
  fi
}

# ── Build ────────────────────────────────────────────────────────────────────

sha256_of() {
  local file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "${file}" | awk '{ print $1 }'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "${file}" | awk '{ print $1 }'
  else
    echo "Error: neither sha256sum nor shasum is available" >&2
    exit 1
  fi
}

# Build both contracts and fail if either artifact is missing or oversized.
build_contracts() {
  require_cmd stellar
  echo "Building WASM (target ${WASM_TARGET})..."
  (cd "${REPO_ROOT}" && stellar contract build)

  local wasm
  for wasm in "${FACTORY_WASM}" "${INSTANCE_WASM}"; do
    if [[ ! -f "${wasm}" ]]; then
      echo "Error: expected artifact not found at ${wasm}" >&2
      echo "       Check that 'stellar contract build' targets ${WASM_TARGET}." >&2
      exit 1
    fi
  done

  check_wasm_sizes
}

# Enforce the 128KB per-artifact limit on the files that actually get deployed.
check_wasm_sizes() {
  local failed=0 wasm size
  for wasm in "${FACTORY_WASM}" "${INSTANCE_WASM}"; do
    size="$(wc -c < "${wasm}" | tr -d ' ')"
    echo "$(basename "${wasm}"): ${size} bytes"
    if [[ "${size}" -gt "${MAX_WASM_SIZE}" ]]; then
      echo "Error: $(basename "${wasm}") exceeds the ${MAX_WASM_SIZE}-byte limit" >&2
      failed=1
    fi
  done
  return "${failed}"
}

# ── Deployment ───────────────────────────────────────────────────────────────

# Refuse to overwrite an existing, initialised deployment unless the caller
# opts in with ALLOW_REDEPLOY=1 (issue #842 — re-running must be safe).
require_no_existing_deployment() {
  local network="$1"
  local manifest="${REPO_ROOT}/deployments/${network}.json"

  [[ -f "${manifest}" ]] || return 0
  [[ "${ALLOW_REDEPLOY:-0}" == "1" ]] && return 0

  local existing
  existing="$(json_field "${manifest}" contractId)"
  [[ -n "${existing}" ]] || return 0

  echo "Error: ${network} already has a recorded deployment at ${existing}." >&2
  echo "       Deploying again creates a second factory and orphans the first." >&2
  echo "       Re-run with ALLOW_REDEPLOY=1 if that is what you want." >&2
  exit 1
}

# Read one top-level string field out of a JSON file without requiring jq.
json_field() {
  local file="$1" key="$2"
  [[ -f "${file}" ]] || return 0
  sed -n "s/.*\"${key}\"[[:space:]]*:[[:space:]]*\"\([^\"]*\)\".*/\1/p" "${file}" | head -1
}

# Install the instance WASM, deploy the factory, initialise it, and verify.
#
# Sets: INSTANCE_WASM_HASH, FACTORY_WASM_HASH, FACTORY_CONTRACT_ID.
deploy_and_init_factory() {
  local network="$1" source_key="$2" admin="$3" treasury="$4" protocol_fee_bp="$5"

  # 1. Upload the instance WASM. The factory stores this hash as
  #    DataKey::InstanceWasmHash and cannot create a raffle without it.
  echo "Installing raffle-instance WASM..."
  INSTANCE_WASM_HASH="$(stellar contract install \
    --wasm "${INSTANCE_WASM}" \
    --source "${source_key}" \
    --network "${network}")"
  echo "Instance WASM hash: ${INSTANCE_WASM_HASH}"

  # 2. Deploy the factory.
  echo "Deploying raffle-factory to ${network}..."
  FACTORY_CONTRACT_ID="$(stellar contract deploy \
    --wasm "${FACTORY_WASM}" \
    --source "${source_key}" \
    --network "${network}")"
  echo "Factory contract ID: ${FACTORY_CONTRACT_ID}"

  FACTORY_WASM_HASH="$(sha256_of "${FACTORY_WASM}")"

  # 3. Initialise it. Without this the factory is deployed but unusable, which
  #    is exactly the half-finished state issue #842 describes.
  echo "Initialising factory..."
  stellar contract invoke \
    --id "${FACTORY_CONTRACT_ID}" \
    --source "${source_key}" \
    --network "${network}" \
    -- \
    init_factory \
    --admin "${admin}" \
    --wasm_hash "${INSTANCE_WASM_HASH}" \
    --protocol_fee_bp "${protocol_fee_bp}" \
    --treasury "${treasury}"

  # 4. Prove it took, rather than trusting the invoke's exit code.
  verify_factory_initialised "${network}" "${source_key}" "${admin}"
}

# Read the factory's admin back and confirm it matches what we just set.
verify_factory_initialised() {
  local network="$1" source_key="$2" expected_admin="$3"
  local reported

  echo "Verifying factory initialisation..."
  reported="$(stellar contract invoke \
    --id "${FACTORY_CONTRACT_ID}" \
    --source "${source_key}" \
    --network "${network}" \
    -- \
    get_admin | tr -d '"')"

  if [[ "${reported}" != "${expected_admin}" ]]; then
    echo "Error: factory reports admin ${reported}, expected ${expected_admin}" >&2
    exit 1
  fi
  echo "Factory is initialised and reports admin ${reported}."
}

# Compare the deployed bytecode against the local artifact (issue #843).
verify_deployed_bytecode() {
  local network="$1" contract_id="$2"
  echo "Verifying deployed bytecode..."
  RAFFLE_CONTRACT_ADDRESS="${contract_id}" STELLAR_NETWORK="${network}" \
    "${SCRIPT_DIR}/verify.sh"
}

# ── Deployment manifest ──────────────────────────────────────────────────────

# Write deployments/<network>.json and append the same record to
# deployments/<network>-history.jsonl so history is never overwritten (#842).
write_deployment_manifest() {
  local network="$1" admin="$2" treasury="$3" protocol_fee_bp="$4"
  local dir="${REPO_ROOT}/deployments"
  local timestamp git_commit git_dirty record

  mkdir -p "${dir}"
  timestamp="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
  git_commit="$(git -C "${REPO_ROOT}" rev-parse HEAD 2>/dev/null || echo unknown)"
  if [[ -n "$(git -C "${REPO_ROOT}" status --porcelain 2>/dev/null)" ]]; then
    git_dirty=true
  else
    git_dirty=false
  fi

  read -r -d '' record <<EOF || true
{
  "network": "${network}",
  "contractId": "${FACTORY_CONTRACT_ID}",
  "factoryWasmHash": "${FACTORY_WASM_HASH}",
  "instanceWasmHash": "${INSTANCE_WASM_HASH}",
  "admin": "${admin}",
  "treasury": "${treasury}",
  "protocolFeeBp": ${protocol_fee_bp},
  "gitCommit": "${git_commit}",
  "gitDirty": ${git_dirty},
  "wasmTarget": "${WASM_TARGET}",
  "rustToolchain": "$(rust_toolchain_channel)",
  "stellarCli": "$(stellar_cli_version)",
  "timestamp": "${timestamp}"
}
EOF

  printf '%s\n' "${record}" > "${dir}/${network}.json"
  printf '%s\n' "${record}" | tr -d '\n ' >> "${dir}/${network}-history.jsonl"
  printf '\n' >> "${dir}/${network}-history.jsonl"

  echo "Wrote ${dir}/${network}.json"
  echo "Appended to ${dir}/${network}-history.jsonl"
}
