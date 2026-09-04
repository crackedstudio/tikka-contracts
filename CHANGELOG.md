# Changelog

All notable changes to this project are documented here.  
Format: [Keep a Changelog](https://keepachangelog.com/en/1.0.0/)  
Versioning: [Semantic Versioning](https://semver.org/)

## [Unreleased]

### Added

- Contributor docs: `docs/DEPLOYMENT.md`, `docs/STORAGE.md`, `docs/RANDOMNESS.md`, and `docs/FAQ.md`.
- Architecture documentation with factory -> instance -> oracle flow and state-machine diagrams (`docs/ARCHITECTURE.md`).
- Comprehensive rustdoc comments for all public `raffle-shared` enums, structs, fields, constants, and functions.
- Pull request template requiring changelog updates for non-trivial changes.
- `OracleSeedDelivered` event documenting per-oracle quorum seed submissions; `OracleNotRegistered` and `DuplicateOracleSubmission` error codes (#856).
- Deterministic event-doc generator (`scripts/generate_event_docs.py`) with a CI sync check for `docs/EVENTS.md` (#856).
- Rust line-coverage ratchet (`scripts/check_coverage_ratchet.py`) with `coverage/coverage-ratchet.json` baseline, a `coverage` CI job, and oracle coverage artifact upload (#830).

### Changed

- README documentation section now links to architecture docs.
- `Error::ExceedsMaxTicketsPerAddress` renumbered from `65` to `67` (was colliding with `CancelTimelockActive`); `OracleNotRegistered` (`68`) and `DuplicateOracleSubmission` (`69`) added (#605, #856).
- `WinnerDrawn.ticket_id` now publishes the 1-based ticket ID (`index + 1`) instead of the 0-based pool position; `docs/EVENTS.md` documents the index-vs-ID convention (#856).
- `TicketPurchased.effective_ticket_price` is now populated (was an undefined variable that broke the build); `TicketGifted` sets it consistently (#856).
- `ContractPaused` / `ContractUnpaused` event structs defined once in `raffle-shared` and re-exported, removing the duplicate E0252 definitions in the factory and instance crates (#856).
- `.github/CODEOWNERS` now uses real maintainer usernames instead of the nonexistent `@maintainers` team (#848).
- `generate_error_docs.py` regenerates the whole `docs/ERRORS.md` deterministically from both the instance and factory error enums (#856).

### Documented

- Standardized event emission model and event catalog (`docs/EVENTS.md`).
- All `#[contractevent]` structs and fields now carry `///` doc comments with explicit index-vs-ID semantics; `max_tickets_per_address` doc entries in `init.rs` and `tickets.rs` updated to reflect that the cap is now validated and enforced (#605, #856).
- Lifecycle/admin event coverage and event publishing patterns from the previous implementation summary.
- Admin key migration was recorded as a historical note (source file existed but contained no additional details).
#### Factory contract
- Two-step admin transfer (`transfer_factory_admin` / `accept_factory_admin`) with `AdminTransferProposed` and `AdminTransferAccepted` events (#87, #339).
- Five tests for factory admin two-step transfer flow: propose+accept, rejects duplicate proposals, wrong address rejection, self-proposal clears pending entry, only pending admin can accept (#453, #545).
- Timelocked admin-config operations (`set_config` / `execute_config_change` / `cancel_config_change`) using a 48-hour `TIMELOCK_DELAY_SECONDS` constant and `AdminOp` enum (#515).
- Per-creator raffle index (`DataKey::CreatorRaffles`) and `get_raffles_by_creator` paginated query (#533).
- Per-category raffle index (`DataKey::CategoryRaffles`) and `get_raffles_by_category` paginated query (#439, #653).
- `get_raffle_by_id` and `get_next_raffle_id` O(1) stable-ID lookups replacing the former `Vec`-based raffle list.
- `get_protocol_stats` returning total raffles created, fee, pause status, and unique participants.
- `set_creation_delay` / `set_whitelist_status` for creation rate-limiting and partner whitelisting (#447).
- `CreationRateLimited` event emitted when non-whitelisted creator hits the cooldown window (#447).
- Periodic `StateCheckpoint` snapshots every `CHECKPOINT_INTERVAL` (1000) raffles with `get_checkpoint` and `get_latest_checkpoint_index` accessors.
- `rescue_tokens` on the factory to recover accidentally sent tokens (#534).
- `upgrade` function with `FactoryUpgraded` event for on-chain WASM upgrades.
- Factory relay helpers `sync_admin`, `pause_instance`, and `unpause_instance` to propagate settings to instances (#87).
- Re-initialization guard (`DataKey::Initialized`); repeat `init_factory` calls are rejected (#288).
- Zero-address and self-reference validation for admin and treasury in `init_factory` and `set_config` (#500).
- `DataKey::InvalidTreasury` error variant; treasury must be an account address, not a contract address (#241).
- `get_total_volume` / `record_volume` for per-asset volume tracking across all raffles.
- Regression test confirming `FairnessData` output format is stable (#532).
- Boundary tests for `get_raffles_page` pagination edge cases (#533).

#### Instance contract
- `NftTicketTrait` cross-contract interface in `raffle-shared`; raffle instance calls `mint` on a configured NFT contract after each successful ticket purchase (#83).
- Early-bird pricing: optional `early_bird_ticket_percentage` and `early_bird_discount_bp` basis-point fields on `RaffleConfig`; discounted price applied while the early-bird quota has not been exhausted (#519).
- Optional raffle category/tag field (`category: Option<String>`) on `RaffleConfig`; validated to ≤ 32 bytes, ASCII alphanumerics and hyphens only (#439).
- `PendingPrize` status: raffle starts in `PendingPrize` and transitions to `Active` only after prize is deposited (#225).
- `RaffleFailed` event and `FailureReason` enum (`ZeroTicketsSold`, `MinTicketsNotMet`) to distinguish failures from explicit cancellations (#232).
- `batch_refund_tickets` for efficient mass refunds on cancelled raffles (#512).
- `submit_commit` for commit-reveal randomness path; commits keyed by ticket ID (not owner address) so they survive ticket transfers (#311, #387).
- `TicketNftMinted` event emitted after each successful `mint` cross-contract call.
- `OwnerTickets(Address)` storage key for O(1) per-owner ticket-ID index.
- `DrawingLock` reentrancy guard on finalization to prevent concurrent draw corruption (#396).
- `PendingAdminCancel` storage key and 48-hour timelock for admin-initiated cancellations of raffles with tickets sold; immediate cancellation still allowed when zero tickets are sold (#406, #543).
- `execute_admin_cancel` to run the delayed admin cancel after the timelock elapses.
- `get_pending_cancel` getter exposing the scheduled cancel timestamp.
- `CancelScheduled` event emitted when an admin cancel is timelocked.
- `ticket_sales_paused` flag with `pause_ticket_sales` / `resume_ticket_sales` functions and corresponding events (#515).
- `no_deadline` flag in `RaffleConfig`; raffles without a hard end timestamp stay open until all tickets sell (#357).
- Configurable `swap_deadline_seconds` (default 300 s, max 3600 s) replacing the hardcoded 300-second swap deadline (#272, #386, #516).
- `set_swap_deadline` admin function with `SwapDeadlineUpdated` event.
- Per-wallet ticket purchase limit (`max_tickets_per_tx`) enforced in `buy_tickets` (#266, #542).
- Per-wallet ownership cap enforced across concurrent purchases (#280).
- `prize_token` field on `Raffle` struct; defaults to `payment_token` but can be overridden (#344).
- Minimum ticket price constant `MIN_TICKET_PRICE` (10 000 stroops) to prevent dust raffles.
- Maximum prize amount constant `MAX_PRIZE_AMOUNT` (1 × 10²¹) to prevent overflow (#376).
- `MAX_PRIZES` (100) cap on prize-tier count (#372).
- `update_oracle_address` admin function with `OracleAddressUpdated` event for rotating the oracle on active raffles (#195).
- Oracle address validated at `init` time for `ExternalRandomness` raffles: rejects the zero address, self-reference, and non-external configs (#276).
- Oracle pending-flag stored in persistent (not instance) storage (#353).
- Minimum ledger delay (`RANDOMNESS_MIN_DELAY_LEDGERS = 10`) before oracle randomness can be fulfilled, preventing same-ledger manipulation (#514).
- `trigger_randomness_fallback` with `FallbackTooEarly` guard; `do_refund=false` path finalizes via internal PRNG, `do_refund=true` cancels with full refunds (#443, #539).
- VRF proof bound to raffle contract address and request context to prevent cross-raffle signature replay (#410, #523).
- `CommitRevealEntry { committer, hash }` struct keyed by `DataKey::CommitEntry(u32)` (ticket ID).
- `AccumulatedFees` instance storage key; `withdraw_fees` requires `Finalized` or `Claimed` status (#367).
- `withdraw_fees` finalized-state guard (#367).
- `emergency_withdraw` with 90-day delay post-finalization; respects `no_deadline` flag by using `RandomnessRequestLedger` instead of `end_time` for the delay (#368, #407).
- `rescue_tokens` on the instance to recover accidentally sent non-prize tokens.
- `wipe_storage` clears all per-ticket and per-owner keys after a terminal state, cleaning up `PendingAdminCancel` as well.
- `TicketBuyers` persistent key tracking all buyers for efficient storage wipe.
- `require_valid_role_address` zero-address + self-reference validation in `set_admin` (#195).
- Seven domain submodules (`admin`, `claim`, `draw`, `helpers`, `init`, `tickets`, `views`) extracted from the monolithic `lib.rs`; each submodule is < 300 lines (#536).
- Three oracle timeout-fallback tests: fallback-with-refund cancels, fallback-without-refund uses internal seed, fallback-too-early is rejected (#443, #539).
- Full lifecycle happy-path test: init → deposit → 3 buys → finalize → claim, asserting balances, status transitions, treasury fees, and `total_distributed == prize_amount` (#440).
- `buy_tickets` budget test asserting final 100-ticket batch stays within Soroban CPU/memory limits (#449).
- Commit-reveal randomness path coverage (#302).
- Raffle invariants and `DrawingLock` coverage tests; oracle startup config validation (#422, #442, #444, #446).

#### raffle-shared crate
- `NftTicketTrait` contract client trait (`NftTicketClient`) for cross-contract NFT minting.
- `FailureReason` enum (`ZeroTicketsSold`, `MinTicketsNotMet`).
- `FairnessData` struct with `seed`, `randomness_source`, `ticket_ids`, `winning_ticket_indices`, `draw_timestamp`, `draw_sequence`.
- `PaginationParams`, `PageResultRaffles`, `PageResultTickets` types for paginated queries.
- `AdminOp` enum (`SetConfig`, `UpdateWasmHash`) for timelocked operations.
- `RandomnessType` enum (`Prng`, `Vrf`, `Fallback`).
- `impl_require_admin!` and `impl_require_not_paused!` macros shared across factory and instance.
- Protocol-wide constants module (`raffle_shared::constants`): `ORACLE_TIMEOUT_LEDGERS`, `MAX_DESCRIPTION_LENGTH`, `MAX_TICKETS_LIMIT`, `MAX_PRIZES`, `MAX_CATEGORY_LENGTH`, `MIN_TICKET_PRICE`, `MAX_PRIZE_AMOUNT`, `DEFAULT_CLAIM_LOCKUP_SECONDS`, `MAX_CLAIM_LOCKUP_SECONDS`, `DEFAULT_SWAP_DEADLINE_SECONDS`, `MAX_SWAP_DEADLINE_SECONDS`, `EMERGENCY_WITHDRAW_DELAY_SECONDS`, `TIMELOCK_DELAY_SECONDS`, `CHECKPOINT_INTERVAL`, `MAX_PROTOCOL_FEE_BP`, `DEFAULT_PAGE_LIMIT`, `MAX_PAGE_LIMIT`.
- Comprehensive rustdoc comments for all public enums, structs, fields, constants, and functions.
- `MAX_CATEGORY_LENGTH` (32) constant for raffle category validation (#439).

#### Oracle service (TypeScript)
- `TxSubmitterService` with `SorobanRpc`, exponential-backoff retry, sequence management, and confirmation polling (#416).
- `KeyService` with startup validation, KMS/Vault adapter stubs, and secret zeroization on shutdown (#415).
- Event listener polling `RandomnessRequested` events from the Soroban RPC; stores ledger checkpoint; enqueues oracle work (#418).
- ESLint and Prettier added to the oracle service (#522).
- VRF proof message bound to raffle address and request context to prevent cross-raffle proof replay (#410).

#### CI / tooling
- Automated release pipeline (`.github/workflows/release.yml`) triggered on `v*.*.*` semver tags; builds `raffle.wasm` and `raffle_instance.wasm` and attaches them to the GitHub release (#467, #518).
- WASM binary size check in CI failing the build if any artifact exceeds the 128 KB Soroban limit (#465, #505).
- `cargo deny` for license compliance and dependency vetting (#469, #506).
- `CODEOWNERS` file assigning review responsibilities (#665).
- `docs/README.md` index linking to all documentation files (#665).
- `workflow_dispatch` trigger on CI workflow to allow manual runs on fork branches.

### Changed

- Crate renamed from `raffle` to `raffle-factory` to match its actual role (#541).
- `Contract` struct in `raffle-instance` renamed to `RaffleInstance` for clarity (#540).
- `get_raffles` renamed to `get_raffles_page` for consistency with paginated query naming conventions (#373).
- Raffle storage migrated from `Vec`-based list to O(1) stable-ID map (`DataKey::RaffleById(u32)` + `DataKey::NextRaffleId`) so read and write costs are constant regardless of raffle count.
- `RandomnessSeed` storage tier changed to persistent (was instance) to survive ledger entry expiry (#383).
- Protocol fee charged at ticket purchase only; removed duplicate deduction at `claim_prize` (#411).
- `finalize_raffle` skips `transition_to_drawing` when status is already `Drawing` (set by `buy_tickets` on sell-out); removed the redundant top-level `DrawingLock` guard that blocked legitimate finalizations.
- `DrawingLock` reentrancy guard is now a separate `DataKey::DrawingLock` instance-storage flag rather than inline logic.
- Commit-reveal entries keyed by ticket ID (`DataKey::CommitEntry(u32)`) instead of owner address so commits survive ticket transfers (#311).
- `PrngWinnerSelection` and `OracleSeedWinnerSelection` extracted as implementations of the `WinnerSelectionStrategy` trait (#537).
- Internal PRNG seed construction uses four base inputs in `build_internal_seed`; `PrngWinnerSelection` adds tickets sold in a second hash.
- `require_valid_role_address` uses an XDR-based zero-contract check (WASM-compatible) instead of `address.exists()` (#520).
- `.gitignore` updated to exclude `target/`, `*.wasm`, `Cargo.lock`, `oracle/node_modules/`, `oracle/dist/`, `.stellar/`, `.soroban/`, `deployments/mainnet.json`, `.env`, `.DS_Store` (#499, #544).
- `Cargo.toml` files for all crates now include `license`, `description`, `repository`, `authors`, `keywords`, and `categories` metadata (#493, #525).
- `rust-version` (MSRV) added to workspace `Cargo.toml` (#309, #388).
- `actions/checkout` upgraded from v3 to v4 across all workflow files (#464, #517, #520).
- Project-wide `.editorconfig` added for consistent editor settings (#542).

### Fixed

- `set_admin` now rejects zero address and self-assignment to prevent admin lockout (#394, #520).
- `init_factory` now validates admin and treasury addresses against zero and self-reference (#500).
- `finalize_raffle` DrawingLock reentry bug fixed: lock is now set correctly before and cleared after the draw (#396, #520).
- `refund_ticket` double reentrancy guard removed; single `Guard` RAII wrapper used after status check (#397).
- `record_volume` guarded with `checked_add` to prevent total-volume overflow (#362).
- `claim_prize` deducts fee only at purchase, not again at claim (#411).
- Oracle `request_id` collision prevented by including raffle address in the hash (#262).
- `sync_admin` now uses `try_invoke_contract` so errors propagate rather than panic (#261).
- `emergency_withdraw` respects `no_deadline` flag in the `Drawing` path by using `RandomnessRequestLedger` for the delay (#407).
- Oracle verification payload uses XDR context (raffle address + request ID) to prevent signature replay attacks (#410, #523).
- Oracle pending-flag stored in persistent storage, preventing it from expiring during a long oracle wait (#353).
- `refund_ticket` correctly decrements the buyer ticket count (#282, #324).
- `prize_net_amount` subtraction guarded against underflow (#281, #327).
- `withdraw_fees` requires `Finalized` or `Claimed` status (#367).
- `buy_tickets` enforces `max_tickets` and `allow_multiple` under concurrent purchases (#177, #208).
- Ticket counter kept consistent with a persistent `NextTicketId` storage key (#319).
- `TicketPurchased` event now includes the correct `raffle_id` field (#341).
- `RaffleFinalized` event emission added to `provide_randomness` path where it was previously missing (#347).
- Raffle creation rejected when `end_time` is in the past (#274, #390).
- `RandomnessSeed` changed to persistent storage so it survives ledger-entry expiry (#383).
- Minimum ledger delay enforced before oracle randomness can be fulfilled (#514).
- Protocol fee upper bound check added to `set_config` (#301, #374).
- `propose_admin` rejects burnt/unowned addresses via zero-address validation (#371).
- `admin-key-change` historical event recorded in `Admin-key-change.md` (no contract changes).
- `RaffleStatus::Finalizing` dead enum variant removed; Drawing-to-Finalized transition is atomic (#403, #526).
- `require_registered_raffle` dead factory helper removed; incorrect auth semantics documented in `MIGRATION-426.md` (#412, #526).
- Instance `rescue_tokens` prevents withdrawing the prize token while prize is deposited.
- `TxSubmitterService` TypeScript compile errors resolved (#416).
- Error enum spacing made consistent across `Error` variants (#524).

### Documented

- `docs/DEPLOYMENT.md`: step-by-step deploy flow using `scripts/` for testnet and mainnet (#569, #656).
- `docs/STORAGE.md`: storage key layout, tier selection rationale, and TTL bump policies for both contracts (#570, #656).
- `docs/RANDOMNESS.md`: tradeoffs between Internal, External, and CommitReveal modes with threat model (#571, #656).
- `docs/FAQ.md`: contributor setup troubleshooting, common build errors, and development environment tips (#572, #656).
- `docs/ARCHITECTURE.md`: factory → instance → oracle flow and raffle lifecycle state-machine diagrams (#513).
- `docs/COMMIT_REVEAL.md`: multi-phase commit-reveal protocol specification with code examples (#521).
- `docs/EVENTS.md`: complete on-chain event catalog for all 44+ events, enum value tables, indexer notes, and emission conditions (#504).
- `docs/FEE_MODEL.md`: two-stage fee collection model and revenue distribution (#530).
- `docs/MIGRATION-426.md`: storage layout migration guide for PR #426, including removal of dead code (#526).
- `docs/README.md`: documentation index linking to all guides (#665).
- `CODEOWNERS`: review responsibility assignments (#665).
- `README.md` updated with current crate names, documentation links, and `metadata_hash` usage guide.
- Comprehensive rustdoc on all public `raffle-shared` types, constants, and functions.

## [0.2.0] - 2025-01-01

### Added

- Commit-reveal randomness source.
- Max tickets per transaction cap.
- Claim lockup delay configuration.
- Drawing/finalization guard state.

### Fixed

- Admin zero-address validation in `set_admin`.
- Duplicate winner selection in oracle finalization path.
