//! Shared constants used by both the factory and instance crates.
//!
//! All magic numbers, tunable parameters, fee bounds, page limits, timeouts,
//! timelock durations, and security thresholds belong here — **never inline a
//! numeric constant in a handler or validation function**.  When a value needs
//! to be adjusted, it is changed here once and both crates see the update.
//!
//! If a constant is ONLY used inside a single crate and will never be needed
//! elsewhere, place it in `<crate>/src/constants.rs` instead.  Anything that
//! must be in sync across crates (e.g. raffle config bounds) goes in THIS file.

/// Default page limit for paginated queries (`get_raffles_page`, etc.)
/// when the caller passes `limit = 0`.
pub const DEFAULT_PAGE_LIMIT: u32 = 100;

/// Hard upper bound on a single page.  Callers that request more than this
/// are silently capped to avoid blowing up per-tx instructions with a huge
/// vector loop.
pub const MAX_PAGE_LIMIT: u32 = 200;

/// Oracle timeout, expressed in **ledgers** (~5 s each on mainnet).  After
/// this many ledgers have elapsed since `RandomnessRequestLedger`, anyone can
/// invoke `trigger_randomness_fallback` to either cancel the raffle or fall
/// back to the on-chain PRNG path.
///
/// 200 ledgers ≈ 17 minutes.
pub const ORACLE_TIMEOUT_LEDGERS: u32 = 200;

/// Maximum number of characters permitted in a raffle description.  Bounded
/// because the description is copied into the on-chain `Raffle` struct that
/// lives in instance storage; an unbounded string would allow a creator to
/// bloat the instance entry and waste XLM.
pub const MAX_DESCRIPTION_LENGTH: u32 = 1_000;

/// Absolute ceiling on `max_tickets` to keep the ticket array + winner
/// selection loop tractable.  Above this value a single draw could exceed
/// the instruction budget.
pub const MAX_TICKETS_LIMIT: u32 = 100_000;

/// The maximum number of prize tiers a single raffle may declare.  More
/// tiers = more work in winner selection + more per-tier loops in
/// `do_finalize_with_seed` and `calculate_tier_prize`.
pub const MAX_PRIZES: u32 = 100;

/// Floor on the per-ticket price in the native (unscaled) token units.
/// Tickets cannot be free or dust-priced; this also guards against div-by-0
/// fee math.  10_000 stroops = 0.00001 of a typical 7-decimal asset.
pub const MIN_TICKET_PRICE: i128 = 10_000;

/// Cap on the total prize pool to bound the amount of escrowed value a
/// single instance can hold.  Expressed in the raw (unscaled) token units.
/// 1e21 = 1e14 in a 7-decimal asset — a deliberately generous ceiling that
/// still prevents silly/typo inputs.
pub const MAX_PRIZE_AMOUNT: i128 = 1_000_000_000_000_000_000_000;

/// Default claim lockup (seconds) if `claim_lockup_seconds == 0` on init.
/// A 1-hour window gives indexers and validators time to flag suspicious
/// draws (e.g., oracle manipulation) before the winner can cash out.
pub const DEFAULT_CLAIM_LOCKUP_SECONDS: u64 = 3_600;

/// Absolute maximum claim lockup (seconds) — the creator cannot lock the
/// winner out for longer than 7 days.
pub const MAX_CLAIM_LOCKUP_SECONDS: u64 = 604_800;

/// Emergency withdraw waiting period.  After finalization (or, for a stuck
/// Drawing raffle, after `end_time + this delay`), the creator + admin may
/// invoke `emergency_withdraw` to pull the prize back out-of-band.  Set
/// long enough that a legitimate winner has every chance to claim first.
/// 90 days in seconds.
pub const EMERGENCY_WITHDRAW_DELAY_SECONDS: u64 = 90 * 24 * 3_600;

/// Timelock delay on factory admin operations (`set_config`,
/// `UpdateWasmHash`).  An op proposed via `set_config` cannot be executed
/// via `execute_config_change` until this many seconds have elapsed.
/// 48 hours in seconds.
pub const TIMELOCK_DELAY_SECONDS: u64 = 172_800;

/// Checkpoint cadence: a new state checkpoint is published every time the
/// running total of raffles created crosses a multiple of this value.
/// 1_000 = roughly one checkpoint per 1k raffles.
pub const CHECKPOINT_INTERVAL: u32 = 1_000;

/// Maximum protocol fee expressed in basis points (1 bp = 0.01%).
/// Both factory-level (default for newly created raffles) and instance-level
/// (per-raffle admin adjustments) fee configs are clamped to this bound.
/// 2_000 bp = 20%.
pub const MAX_PROTOCOL_FEE_BP: u32 = 2_000;
