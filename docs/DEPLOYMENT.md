# Deployment Guide

This document covers the `scripts/` deployment toolchain and the `deployments/` registry. A contributor can fund an account, deploy the factory to testnet, initialize it, create a raffle, invoke functions, and verify the on-chain WASM using only the steps below.

## Prerequisites

- [rustup](https://rustup.rs/) — the compiler version and the WASM target are
  pinned in `rust-toolchain.toml` and installed automatically
- Stellar CLI **23.4.1** — pinned as `STELLAR_CLI_VERSION` in `scripts/common.sh`
- A funded Stellar account secret key for the target network

```bash
rustup show                 # installs the pinned toolchain + wasm32v1-none

cargo install --locked stellar-cli --version 23.4.1 --features opt
stellar --version           # expect 23.4.1
```

### Build target

Everything builds `wasm32v1-none` — the target `stellar contract build` produces
and the only target Soroban accepts. The Makefile, CI, the deploy scripts and
`verify.sh` all derive the target and the artifact paths from a single place,
`scripts/common.sh`:

```bash
WASM_TARGET="wasm32v1-none"
WASM_DIR="${REPO_ROOT}/target/${WASM_TARGET}/release"
FACTORY_WASM="${WASM_DIR}/raffle_factory.wasm"
INSTANCE_WASM="${WASM_DIR}/raffle_instance.wasm"
```

Change it there and every path follows. Do not hardcode a target or an artifact
path anywhere else.

## Environment variables

Copy the example env file and fill in the values you need:

```bash
cp .env.example .env
```

Every script under `scripts/` loads `.env` when present (`export $(cat .env | xargs)`).

| Variable                               | Required by                | Purpose                                        |
| -------------------------------------- | -------------------------- | ---------------------------------------------- |
| `DEPLOYER_SECRET_KEY`                  | `deploy-*.sh`, `invoke.sh` | Account that signs deploy/invoke txs (`S...`)  |
| `RAFFLE_CONTRACT_ADDRESS`              | `invoke.sh`, `verify.sh`   | Contract ID to invoke or verify (`C...`)       |
| `STELLAR_NETWORK`                      | `invoke.sh`, `verify.sh`   | Network name (`testnet` default, or `mainnet`) |
| `STELLAR_RPC_URL`                      | oracle / manual CLI        | Soroban RPC endpoint                           |
| `STELLAR_HORIZON_URL`                  | optional                   | Horizon endpoint                               |
| `FACTORY_CONTRACT_ID`                  | oracle service             | Factory ID the oracle listens on               |
| `ORACLE_ADDRESS` / `ORACLE_SECRET_KEY` | oracle / External raffles  | Oracle identity for `provide_randomness`       |
| `ADMIN_ADDRESS`                        | `deploy-*.sh`              | Factory admin G-address (**required**)         |
| `TREASURY_ADDRESS`                     | `deploy-*.sh`              | Treasury G-address; defaults to `ADMIN_ADDRESS` |
| `PROTOCOL_FEE_BP`                      | `deploy-*.sh`              | Protocol fee in basis points; defaults to `0`  |
| `ALLOW_REDEPLOY`                       | `deploy-*.sh`              | Set to `1` to replace a recorded deployment    |

> **Security:** Never commit `.env` or secret keys. `DEPLOYER_SECRET_KEY` and `ORACLE_SECRET_KEY` must stay local or in a secrets manager.

---

## Scripts reference

### `scripts/fund-testnet.sh`

**Purpose:** Request testnet XLM from Friendbot for a public key.

**Required args:** `<stellar_public_key>` (G-address)

**Env vars:** none required

**Example:**

```bash
./scripts/fund-testnet.sh GD....YOUR_PUBLIC_KEY
```

**Expected output:** Friendbot JSON response, then `Funding complete!`

---

### `scripts/deploy-testnet.sh`

**Purpose:** Produce a fully initialised factory on testnet in one command.

The script:

1. Builds both contracts with `stellar contract build` and enforces the 128KB
   size limit on the artifacts it is about to deploy
1. Installs the **raffle-instance** WASM and captures its hash — the factory
   stores this as `DataKey::InstanceWasmHash` and cannot create a raffle without it
1. Deploys the **raffle-factory**
1. Calls `init_factory` with the admin, instance WASM hash, protocol fee and treasury
1. Reads `get_admin` back off the chain to confirm the factory is initialised
1. Runs `verify.sh` to confirm the deployed bytecode matches the local build
1. Writes `deployments/testnet.json` and appends to `deployments/testnet-history.jsonl`

There is no manual step between deploy and usable.

**Required env:** `DEPLOYER_SECRET_KEY`, `ADMIN_ADDRESS`
**Optional env:** `TREASURY_ADDRESS` (defaults to `ADMIN_ADDRESS`), `PROTOCOL_FEE_BP` (defaults to `0`), `ALLOW_REDEPLOY`

**Example:**

```bash
export DEPLOYER_SECRET_KEY="S..."
export ADMIN_ADDRESS="G..."
export PROTOCOL_FEE_BP=250
./scripts/deploy-testnet.sh
```

**Re-running is safe.** If `deployments/testnet.json` already records a contract
ID, the script refuses rather than silently deploying a second factory and
orphaning the first. Pass `ALLOW_REDEPLOY=1` when replacing it is what you want.

---

### `scripts/deploy-mainnet.sh`

**Purpose:** Same sequence as the testnet script, against `mainnet`, writing
`deployments/mainnet.json`.

**Safety:** Prints the admin, treasury and protocol fee, then prompts
`Proceed? (y/N)` and aborts unless you confirm with `y` / `yes`.

**Example:**

```bash
export DEPLOYER_SECRET_KEY="S..."
export ADMIN_ADDRESS="G..."
./scripts/deploy-mainnet.sh
# type y when prompted
```

---

### `scripts/build-reproducible.sh`

**Purpose:** Build both contracts with the pinned toolchain from a clean target
directory and print their SHA-256 hashes. This is the command a third party runs
to check that the bytecode at an address is built from this source.

**Example:**

```bash
./scripts/build-reproducible.sh
```

**Expected output:**

```text
Toolchain
  rust:        1.94.0 (rust-toolchain.toml)
  stellar-cli: 23.4.1 (pinned), 23.4.1 (installed)
  target:      wasm32v1-none
  commit:      <sha>

...

SHA-256
  raffle_factory.wasm   <hex>
  raffle_instance.wasm  <hex>
```

---

### `scripts/invoke.sh`

**Purpose:** Thin wrapper around `stellar contract invoke` for the contract in `RAFFLE_CONTRACT_ADDRESS`.

**Required env:** `RAFFLE_CONTRACT_ADDRESS`, `DEPLOYER_SECRET_KEY`  
**Optional env:** `STELLAR_NETWORK` (default `testnet`)

**Usage:**

```bash
./scripts/invoke.sh <function_name> [args...]
```

**Example:**

```bash
export RAFFLE_CONTRACT_ADDRESS="C..."
export DEPLOYER_SECRET_KEY="S..."
./scripts/invoke.sh get_raffle
```

Arguments after the function name are forwarded to the Stellar CLI as contract function args.

---

### `scripts/verify.sh`

**Purpose:** Fetch the on-chain WASM for `RAFFLE_CONTRACT_ADDRESS` and compare
its SHA-256 hash to the local factory artifact. Both deploy scripts call it
automatically and fail the deployment on a mismatch.

**Required env:** `RAFFLE_CONTRACT_ADDRESS`
**Optional env:** `STELLAR_NETWORK` (default `testnet`)
**Local artifact:** `target/wasm32v1-none/release/raffle_factory.wasm` (build first)

**Example:**

```bash
export RAFFLE_CONTRACT_ADDRESS="C..."
./scripts/build-reproducible.sh
./scripts/verify.sh
```

**Expected output on match:**

```text
Local WASM Hash:  <hex>
Remote WASM Hash: <hex>
Verification Result: Match: YES
```

Exit code `0` on match, `1` on mismatch or fetch failure. The fetched file goes
to a temp path and is removed on exit.

---

## Verifying someone else's deployment

Anyone can check what is running at a recorded address, using only this
repository:

```bash
# 1. Check out the commit the deployment was built from
git checkout "$(jq -r .gitCommit deployments/testnet.json)"

# 2. Rebuild with the pinned toolchain
./scripts/build-reproducible.sh

# 3. Compare against the recorded hashes
jq -r '.factoryWasmHash, .instanceWasmHash' deployments/testnet.json

# 4. Compare against the chain
RAFFLE_CONTRACT_ADDRESS="$(jq -r .contractId deployments/testnet.json)" \
  ./scripts/verify.sh
```

Step 2 must reproduce the hashes from step 3 byte for byte. If it does not,
check that your Stellar CLI matches `STELLAR_CLI_VERSION` in
`scripts/common.sh` and that `gitDirty` is `false` in the manifest.

Every GitHub release carries the same information: the WASM artifacts, a
`SHA256SUMS.txt`, and the toolchain versions in the release notes.

---

## End-to-end testnet flow

Order of operations from a clean machine:

### 1. Configure env

```bash
cp .env.example .env
# Set DEPLOYER_SECRET_KEY and ADMIN_ADDRESS at minimum
```

### 2. Fund the deployer

```bash
stellar keys address <alias-or-secret>
./scripts/fund-testnet.sh <YOUR_PUBLIC_KEY>
```

### 3. Deploy

```bash
./scripts/deploy-testnet.sh
```

That single command builds, installs the instance WASM, deploys the factory,
initialises it, verifies the deployed bytecode, and records the result. The
factory can create raffles when it returns — there is no separate init step.

### 4. Point tooling at the factory

```bash
export RAFFLE_CONTRACT_ADDRESS="$(jq -r .contractId deployments/testnet.json)"
export FACTORY_CONTRACT_ID="$RAFFLE_CONTRACT_ADDRESS"
```

### 5. Upgrade procedure

1. Propose a new instance WASM hash through the factory timelock with `propose_wasm_upgrade`.
1. Confirm the proposal is pending and cannot be executed before `TIMELOCK_DELAY_SECONDS` elapses.
1. Advance the ledger time past the delay, then invoke `execute_config_change` to apply the upgrade.
1. Verify the new instance WASM is active and that existing raffles remain readable after the upgrade.

### 6. Create a raffle

```bash
./scripts/invoke.sh create_raffle \
  --creator "$ADMIN_ADDRESS" \
  --config '<RaffleConfig JSON / XDR per CLI>'
```

`create_raffle` deploys a new **raffle instance** and returns its address. Set `RAFFLE_CONTRACT_ADDRESS` to that instance for ticket/draw calls, or invoke with `--id <instance>` directly via `stellar contract invoke`.

### 7. Fund prize, sell tickets, finalize

Typical instance lifecycle (see [ARCHITECTURE.md](ARCHITECTURE.md)):

1. `deposit_prize` — creator escrows the prize (`PendingPrize` → `Active`)
1. `buy_tickets` — buyers purchase entries
1. `finalize_raffle` — starts the draw (Internal / External / CommitReveal)
1. For `External`: run the `oracle/` service so it calls `provide_randomness`
1. `claim_prize` — winners withdraw after claim lockup

Example finalize:

```bash
export RAFFLE_CONTRACT_ADDRESS="<INSTANCE_C_ADDRESS>"
./scripts/invoke.sh finalize_raffle
```

### 8. Verify factory WASM

```bash
export RAFFLE_CONTRACT_ADDRESS="$(jq -r .contractId deployments/testnet.json)"
./scripts/verify.sh
```

---

## The `deployments/` directory

Each successful deployment writes two files.

`deployments/<network>.json` — the current deployment:

| Field | Meaning |
| --- | --- |
| `network` | `testnet` or `mainnet` |
| `contractId` | Factory contract address |
| `factoryWasmHash` | SHA-256 of the deployed factory artifact |
| `instanceWasmHash` | Hash the factory stores as `DataKey::InstanceWasmHash` |
| `admin` / `treasury` | Values passed to `init_factory` |
| `protocolFeeBp` | Protocol fee in basis points |
| `gitCommit` | Commit the artifacts were built from |
| `gitDirty` | `true` if the tree had uncommitted changes — the build is then not reproducible |
| `wasmTarget` | Build target (`wasm32v1-none`) |
| `rustToolchain` / `stellarCli` | Toolchain versions that produced the bytes |
| `timestamp` | UTC ISO-8601 |

`deployments/<network>-history.jsonl` — one line per deployment, appended and
never rewritten, so replacing a deployment does not erase what came before.

> The existing `deployments/testnet.json` predates this format and records only
> `network`, `contractId` and `timestamp`. The code at
> `CCTCPMI66REXIJQPVOPNTNUZBCMSRM7TZLMIPQROZIID44XNP2P2MKFZ` therefore cannot be
> identified from the repository; the next deployment fills in the rest.

### Recording a new deployment

1. Run `./scripts/deploy-testnet.sh` or `./scripts/deploy-mainnet.sh`.
1. Commit the updated JSON and the history line when the deployment is the
   shared reference address for the team.
1. Update `.env` (`RAFFLE_CONTRACT_ADDRESS` / `FACTORY_CONTRACT_ID`) to match.

The manifest records the **factory** only. Instance addresses returned by
`create_raffle` are tracked separately.

## Related docs

- [DEVELOPMENT.md](DEVELOPMENT.md) — local build and repository workflow
- [ARCHITECTURE.md](ARCHITECTURE.md) — factory → instance → oracle flow
- [RANDOMNESS.md](RANDOMNESS.md) — choose Internal / External / CommitReveal before create
- [FAQ.md](FAQ.md) — CLI naming, WASM targets, Node 20 for `oracle/`
