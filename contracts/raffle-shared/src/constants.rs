// ============================================================================
// Protocol-wide constants
//
// Single source of truth for every magic number used across the raffle
// contracts.  Import from `raffle_shared::constants::*` (or individually) in
// any crate that needs them.
// ============================================================================

// --- Raffle instance limits -------------------------------------------------

/// Maximum number of ledgers the oracle may take to respond before a fallback
/// is permitted (~17 minutes at 5-second ledger close times).
pub const ORACLE_TIMEOUT_LEDGERS: u32 = 200;

/// Maximum byte-length of a raffle description string.
pub const MAX_DESCRIPTION_LENGTH: u32 = 1_000;

/// Hard cap on tickets per raffle.
pub const MAX_TICKETS_LIMIT: u32 = 100_000;

/// Hard cap on the number of prize tiers per raffle.
pub const MAX_PRIZES: u32 = 100;

/// Maximum byte-length of an optional raffle category/tag string (#439).
/// Categories are restricted to ASCII alphanumerics and hyphens.
pub const MAX_CATEGORY_LENGTH: u32 = 32;

/// Minimum ticket price in the payment token's base unit (stroops / smallest
/// denomination).  Prevents dust-amount raffles that would be uneconomical.
pub const MIN_TICKET_PRICE: i128 = 10_000;

/// Maximum allowed prize pool.  Prevents i128 overflow in prize calculations.
pub const MAX_PRIZE_AMOUNT: i128 = 1_000_000_000_000_000_000_000; // 1e21

// --- Timing constants -------------------------------------------------------

/// Default delay (seconds) between raffle finalization and when winners may
/// claim their prize.  Equals 1 hour.
pub const DEFAULT_CLAIM_LOCKUP_SECONDS: u64 = 3_600;

/// Upper bound on the claim lockup delay (7 days).
pub const MAX_CLAIM_LOCKUP_SECONDS: u64 = 604_800;

/// Minimum time (seconds) after finalization before unclaimed prizes may be
/// swept to the treasury.  Equals 30 days.
pub const MIN_CLAIM_EXPIRY_SECONDS: u64 = 30 * 24 * 3_600; // 2_592_000

/// Default claim expiry when the creator does not specify one.
pub const DEFAULT_CLAIM_EXPIRY_SECONDS: u64 = MIN_CLAIM_EXPIRY_SECONDS;

/// Maximum unclaimed prize tiers processed in a single `sweep_unclaimed` call.
pub const MAX_SWEEP_UNCLAIMED_PER_CALL: u32 = 10;

/// Default window (seconds) added to the current timestamp when submitting
/// token-swap transactions.  Equals 5 minutes.
pub const DEFAULT_SWAP_DEADLINE_SECONDS: u64 = 300;

/// Upper bound on the swap deadline window (1 hour).
pub const MAX_SWAP_DEADLINE_SECONDS: u64 = 3_600;

/// Minimum time (seconds) that must elapse after raffle finalization before an
/// emergency withdrawal is permitted.  Equals 90 days (7 776 000 s).
pub const EMERGENCY_WITHDRAW_DELAY_SECONDS: u64 = 90 * 24 * 3_600; // 7_776_000

// --- Factory constants ------------------------------------------------------

/// Timelock delay (seconds) before a proposed admin operation may be executed.
/// Equals 48 hours, giving users time to react to protocol changes.
pub const TIMELOCK_DELAY_SECONDS: u64 = 172_800;

/// Factory creates a state checkpoint every `CHECKPOINT_INTERVAL` raffles.
pub const CHECKPOINT_INTERVAL: u32 = 1_000;

/// Maximum protocol fee in basis points (20 %).
pub const MAX_PROTOCOL_FEE_BP: u32 = 2_000;

// --- Recurring raffe constants ----------------------------------------------

/// Minimum interval between recurring raffle rounds (1 hour).
pub const MIN_RECURRING_INTERVAL_SECONDS: u64 = 3_600;

/// Maximum interval between recurring raffle rounds (365 days).
pub const MAX_RECURRING_INTERVAL_SECONDS: u64 = 31_536_000;

// --- Pagination defaults ----------------------------------------------------

/// Default number of items returned by paginated queries.
pub const DEFAULT_PAGE_LIMIT: u32 = 100;

/// Hard cap on items returned by a single paginated query.
pub const MAX_PAGE_LIMIT: u32 = 200;

// --- TTL bump targets (in ledgers) ------------------------------------------

/// Target TTL ledgers applied to the instance contract entry on every hot-path
/// bump (buy_tickets, finalize_raffle, extend_ttl entrypoint).
///
/// ~6 months at 5 s / ledger: 6 * 30 * 24 * 3600 / 5 = 3 110 400 ledgers.
pub const INSTANCE_TTL_BUMP_LEDGERS: u32 = 3_110_400;

/// Same horizon used as the *threshold* argument to `extend_ttl`: only extend
/// when fewer than this many ledgers remain, avoiding unnecessary writes.
pub const INSTANCE_TTL_THRESHOLD_LEDGERS: u32 = 1_555_200; // ~3 months

/// Target TTL for persistent ticket / commit entries on hot-path bumps.
pub const PERSISTENT_TTL_BUMP_LEDGERS: u32 = 3_110_400;

/// Threshold for persistent entry bumps.
pub const PERSISTENT_TTL_THRESHOLD_LEDGERS: u32 = 1_555_200;
