#![allow(dead_code)]

/// Canonical protocol error catalog.
///
/// This enum exists so that the full Tikka error surface has a single
/// source-of-truth for documentation, off-chain mapping, and future
/// shared error definitions.  It is **not** intended to replace the
/// on-chain `Error` / `ContractError` enums; those keep their existing
/// discriminants so that deployed contracts are not renumbered.
///
/// # Ranges
///
/// | Range     | Owner            | Purpose                                      |
/// | --------- | ---------------- | -------------------------------------------- |
/// | 1 – 99    | Shared           | Conditions used by both instance and factory  |
/// | 100 – 199 | Instance-only    | New instance-specific errors                 |
/// | 200 – 299 | Factory-only     | New factory-specific errors                  |
///
/// # Append-only rule
///
/// Once a code is assigned it is **never reused or reassigned**, even if
/// the variant is later deprecated.  See `CONTRIBUTING.md` for the full
/// policy.
///
/// # Existing overlap
///
/// Instance and factory currently assign different codes to the same
/// concept (e.g. `NotAuthorized` is `5` in the instance and `2` in the
/// factory).  Those codes are preserved below so that no deployed
/// contract is broken.  Going forward, any new shared condition must use
/// a single code in the 1–99 range and both contracts must adopt it.
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum ProtocolError {
    // ------------------------------------------------------------------ --
    // Instance errors — original codes preserved.
    // The instance contract is the source of truth for these discriminants.
    // ------------------------------------------------------------------ --
    RaffleNotFound = 1,
    RaffleInactive = 2,
    TicketsSoldOut = 3,
    InsufficientFunds = 4,
    NotAuthorized = 5,
    OracleNotSet = 6,
    RandomnessAlreadyRequested = 7,
    NoRandomnessRequest = 8,
    FallbackTooEarly = 9,
    PrizeNotDeposited = 11,
    PrizeAlreadyClaimed = 12,
    PrizeAlreadyDeposited = 13,
    NotWinner = 14,
    ClaimTooEarly = 15,
    InvalidParameters = 21,
    InvalidQuantity = 22,
    InvalidStatus = 23,
    ContractPaused = 24,
    InvalidStateTransition = 25,
    RaffleExpired = 26,
    InsufficientTickets = 31,
    MultipleTicketsNotAllowed = 32,
    NoTicketsSold = 33,
    TicketNotFound = 34,
    RaffleEnded = 35,
    ArithmeticOverflow = 41,
    AlreadyInitialized = 42,
    NotInitialized = 43,
    Reentrancy = 44,
    TokenTransferFailed = 45,
    NoActiveTickets = 46,
    DeadlinePassed = 47,
    SlippageExceeded = 48,
    InvalidIndex = 49,
    MorePrizesThanTickets = 50,
    ZeroPrize = 51,
    InvalidTokenAddress = 52,
    TooManyPrizes = 53,
    EmergencyTooEarly = 54,
    InvalidTicketRange = 55,
    InsufficientAccumulatedFees = 56,
    PrizeConfigurationLocked = 57,
    ExceedsMaxTicketsPerTx = 58,
    DrawingAlreadyInProgress = 59,
    DrawingAlreadyComplete = 60,
    InvalidEndTime = 61,
    InvalidAdminAddress = 62,
    InvalidDrawingTransition = 63,
    RandomnessTooEarly = 64,

    // ------------------------------------------------------------------ --
    // Factory errors — mapped to 200+ to avoid conflicts with instance codes.
    // These document the factory's own codes; the original factory codes
    // are noted in comments.  New factory-specific errors must also use 200+.
    // ------------------------------------------------------------------ --
    FactoryAlreadyInitialized = 200,  // factory original: 1
    FactoryNotAuthorized = 201,       // factory original: 2
    FactoryContractPaused = 202,      // factory original: 3
    FactoryInvalidParameters = 203,   // factory original: 4
    FactoryRaffleNotFound = 204,      // factory original: 5
    FactoryAdminTransferPending = 205, // factory original: 11
    FactoryNoPendingTransfer = 206,   // factory original: 12
    FactoryRateLimitExceeded = 207,   // factory original: 13
    FactoryNoPendingOp = 208,         // factory original: 14
    FactoryTimelockNotElapsed = 209,  // factory original: 15
    FactoryInvalidRaffleId = 210,     // factory original: 16
    FactoryRaffleNotEligible = 211,   // factory original: 17
    FactoryArithmeticOverflow = 212,  // factory original: 18
    FactoryTreasuryNotSet = 213,      // factory original: 19

    // ------------------------------------------------------------------ --
    // Reserved for future instance-specific errors (100–199).
    // Factory-specific errors start at 200 and are defined above.
    // Do not assign codes here without updating CONTRIBUTING.md.
    // ------------------------------------------------------------------ --
    ReservedInstance100 = 100,
}
