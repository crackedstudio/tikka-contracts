# Error Codes

This document is **auto-generated** from the contract error enums. **Do not
edit by hand.** Regenerate whenever error codes or descriptions change:

```bash
python scripts/generate_error_docs.py
```

Sources of truth:
- Instance errors: `Error` in
  [`contracts/raffle-instance/src/lib.rs`](contracts/raffle-instance/src/lib.rs).
- Factory errors: `ContractError` in
  [`contracts/raffle-factory/src/lib.rs`](contracts/raffle-factory/src/lib.rs).

---

## Instance Contract Errors

| Code | Error | Description | Frontend Message |
| ---- | ----- | ----------- | ---------------- |
| 1 | `RaffleNotFound` | The raffle data was not found in storage | "Raffle not found" |
| 2 | `RaffleInactive` | The raffle is not in an active state | "This raffle is not currently active" |
| 3 | `TicketsSoldOut` | All tickets have been sold | "Sorry, all tickets have been sold!" |
| 4 | `InsufficientFunds` | User does not have enough balance | "Insufficient funds to complete this action" |
| 5 | `NotAuthorized` | User is not authorized to perform this action | "You are not authorized to perform this action" |
| 6 | `OracleNotSet` | Oracle address is not configured | "Oracle address is not set" |
| 7 | `RandomnessAlreadyRequested` | Randomness has already been requested | "Randomness request already in progress" |
| 8 | `NoRandomnessRequest` | No randomness request found | "No randomness request found" |
| 9 | `FallbackTooEarly` | Fallback randomness triggered too early | "Fallback randomness not available yet" |
| 11 | `PrizeNotDeposited` | Prize has not been deposited yet | "Prize not yet deposited" |
| 12 | `PrizeAlreadyClaimed` | Prize has already been claimed | "Prize has already been claimed" |
| 13 | `PrizeAlreadyDeposited` | Prize deposit was already completed | "Prize has already been deposited" |
| 14 | `NotWinner` | Only the winner can claim the prize | "You are not the winner of this raffle" |
| 15 | `ClaimTooEarly` | Cannot claim before cooldown period | "Please wait before claiming your prize" |
| 21 | `InvalidParameters` | Invalid input parameters provided | "Invalid parameters provided" |
| 22 | `InvalidQuantity` | Invalid ticket quantity requested | "Invalid ticket quantity" |
| 23 | `InvalidStatus` | The current raffle status doesn't allow this operation | "This action is not allowed in the current raffle state" |
| 24 | `ContractPaused` | The contract is paused | "Contract is temporarily paused" |
| 25 | `InvalidStateTransition` | Cannot transition to the requested state | "Cannot change raffle to the requested state" |
| 26 | `RaffleExpired` | The raffle end time has passed | "This raffle has ended" |
| 31 | `InsufficientTickets` | Not enough tickets sold to finalize | "Minimum ticket requirement not met" |
| 32 | `MultipleTicketsNotAllowed` | User already has a ticket | "Multiple tickets not allowed for this raffle" |
| 33 | `NoTicketsSold` | No tickets have been purchased | "No tickets have been sold yet" |
| 34 | `TicketNotFound` | The requested ticket was not found | "Ticket not found" |
| 35 | `RaffleEnded` | The raffle has already ended | "This raffle has already ended" |
| 41 | `ArithmeticOverflow` | Arithmetic operation overflow | "Calculation error occurred" |
| 42 | `AlreadyInitialized` | Contract is already initialized | "Contract already initialized" |
| 43 | `NotInitialized` | Contract has not been initialized | "Contract not initialized" |
| 44 | `Reentrancy` | Reentrant call detected | "Please try again later" |
| 45 | `TokenTransferFailed` | Token transfer failed | "Token transfer failed" |
| 46 | `NoActiveTickets` | No active tickets available | "No active tickets available" |
| 47 | `DeadlinePassed` | Swap deadline has passed | "Swap deadline has passed" |
| 48 | `SlippageExceeded` | Slippage tolerance exceeded | "Slippage tolerance exceeded" |
| 49 | `InvalidIndex` | Invalid index provided | "Invalid index provided" |
| 50 | `MorePrizesThanTickets` | More prizes than tickets | "More prizes than tickets" |
| 51 | `ZeroPrize` | Prize amount is zero | "Prize amount cannot be zero" |
| 52 | `InvalidTokenAddress` | Invalid token address provided | "Invalid token address" |
| 53 | `TooManyPrizes` | Exceeds maximum number of prizes | "Too many prizes configured" |
| 54 | `EmergencyTooEarly` | Emergency withdraw too early | "Emergency withdraw not available yet" |
| 55 | `InvalidTicketRange` | Invalid ticket range configured | "Invalid ticket range" |
| 56 | `InsufficientAccumulatedFees` | Insufficient accumulated fees | "Insufficient accumulated fees" |
| 57 | `PrizeConfigurationLocked` | Prize configuration is locked | "Prize configuration is locked" |
| 58 | `ExceedsMaxTicketsPerTx` | Exceeds max tickets per transaction | "Too many tickets for one transaction" |
| 59 | `DrawingAlreadyInProgress` | A draw is already in progress | "Drawing already in progress" |
| 60 | `InvalidStatusForDrawingTransition` | Raffle status cannot enter Drawing | "Cannot start drawing in current state" |
| 61 | `DrawingAlreadyComplete` | Randomness was already provided | "Drawing already complete" |
| 62 | `InvalidEndTime` | Raffle end time is invalid | "Invalid raffle end time" |
| 63 | `InvalidAdminAddress` | Admin address is invalid | "Invalid admin address" |
| 64 | `RandomnessTooEarly` | Randomness fallback is not available yet | "Randomness fallback not available yet" |
| 65 | `CancelTimelockActive` | Cancellation timelock is still active | "Cancellation timelock is still active" |
| 66 | `CancelNotScheduled` | No cancellation is scheduled | "No cancellation is scheduled" |
| 67 | `ExceedsMaxTicketsPerAddress` | Exceeds the per-address ticket cap | "This address has reached the ticket limit" |
| 68 | `OracleNotRegistered` | Caller is not a registered oracle for this raffle | "Caller is not a registered oracle for this raffle" |
| 69 | `DuplicateOracleSubmission` | This oracle has already submitted its seed | "This oracle has already submitted its seed" |

---

## Factory Contract Errors

| Code | Error | Description | Frontend Message |
| ---- | ----- | ----------- | ---------------- |
| 1 | `AlreadyInitialized` | Factory is already initialized | "Factory already initialized" |
| 2 | `NotAuthorized` | User is not the admin | "You are not the admin" |
| 3 | `ContractPaused` | Factory is paused | "Factory is temporarily paused" |
| 4 | `InvalidParameters` | Invalid parameters provided | "Invalid parameters provided" |
| 5 | `RaffleNotFound` | Raffle instance not found | "Raffle not found" |
| 11 | `AdminTransferPending` | Admin transfer already pending | "Admin transfer already pending" |
| 12 | `NoPendingTransfer` | No pending admin transfer | "No pending admin transfer" |
| 13 | `RateLimitExceeded` | Creator rate limit exceeded | "Rate limit exceeded, try again later" |
| 14 | `NoPendingOp` | No pending admin operation | "No pending admin operation" |
| 15 | `TimelockNotElapsed` | Timelock has not elapsed | "Timelock has not elapsed" |
| 16 | `InvalidRaffleId` | Raffle does not belong to this factory | "Invalid raffle for this factory" |
| 17 | `RaffleNotEligible` | Raffle is not eligible for this operation | "Raffle is not eligible" |
| 18 | `ArithmeticOverflow` | Arithmetic operation overflow | "Calculation error occurred" |
| 19 | `TreasuryNotSet` | Treasury address is not configured | "Treasury address is not set" |
| 20 | `RecurringNotFound` | Recurring raffle schedule not found | "Recurring raffle not found" |
| 21 | `IntervalNotElapsed` | Interval has not elapsed | "Interval has not elapsed" |
| 22 | `MaxRoundsReached` | Maximum rounds reached | "Maximum rounds reached" |
| 23 | `RecurringInactive` | Recurring raffle is inactive | "Recurring raffle is inactive" |
| 24 | `CreationPaused` | Raffle creation is paused | "Raffle creation is paused" |

---

## Error Code Mapping (TypeScript)

```typescript
const errorMessages: Record<number, string> = {
  1: "Raffle not found",
  2: "This raffle is not currently active",
  3: "Sorry, all tickets have been sold!",
  4: "Insufficient funds to complete this action",
  5: "You are not authorized to perform this action",
  6: "Oracle address is not set",
  7: "Randomness request already in progress",
  8: "No randomness request found",
  9: "Fallback randomness not available yet",
  11: "Prize not yet deposited",
  12: "Prize has already been claimed",
  13: "Prize has already been deposited",
  14: "You are not the winner of this raffle",
  15: "Please wait before claiming your prize",
  21: "Invalid parameters provided",
  22: "Invalid ticket quantity",
  23: "This action is not allowed in the current raffle state",
  24: "Contract is temporarily paused",
  25: "Cannot change raffle to the requested state",
  26: "This raffle has ended",
  31: "Minimum ticket requirement not met",
  32: "Multiple tickets not allowed for this raffle",
  33: "No tickets have been sold yet",
  34: "Ticket not found",
  35: "This raffle has already ended",
  41: "Calculation error occurred",
  42: "Contract already initialized",
  43: "Contract not initialized",
  44: "Please try again later",
  45: "Token transfer failed",
  46: "No active tickets available",
  47: "Swap deadline has passed",
  48: "Slippage tolerance exceeded",
  49: "Invalid index provided",
  50: "More prizes than tickets",
  51: "Prize amount cannot be zero",
  52: "Invalid token address",
  53: "Too many prizes configured",
  54: "Emergency withdraw not available yet",
  55: "Invalid ticket range",
  56: "Insufficient accumulated fees",
  57: "Prize configuration is locked",
  58: "Too many tickets for one transaction",
  59: "Drawing already in progress",
  60: "Cannot start drawing in current state",
  61: "Drawing already complete",
  62: "Invalid raffle end time",
  63: "Invalid admin address",
  64: "Randomness fallback not available yet",
  65: "Cancellation timelock is still active",
  66: "No cancellation is scheduled",
  67: "This address has reached the ticket limit",
  68: "Caller is not a registered oracle for this raffle",
  69: "This oracle has already submitted its seed",
};
```
