# Error Codes Documentation

This document describes all error codes used in the Tikka Raffle contracts. Frontend applications can use these codes to display user-friendly error messages.

## Table of Contents

- [Instance Contract Errors](#instance-contract-errors)
- [Factory Contract Errors](#factory-contract-errors)
- [Error Code Mapping](#error-code-mapping)

---

## Instance Contract Errors

The instance contract (`Raffle`) handles individual raffle operations. All error codes are defined in the `Error` enum in [`contracts/raffle-instance/src/lib.rs`](../contracts/raffle-instance/src/lib.rs).

### General Errors (1-10)

| Code | Error               | Description                                   | Frontend Message                                |
| ---- | ------------------- | --------------------------------------------- | ----------------------------------------------- |
| 1    | `RaffleNotFound`    | The raffle data was not found in storage      | "Raffle not found"                              |
| 2    | `RaffleInactive`    | The raffle is not in an active state          | "This raffle is not currently active"           |
| 3    | `TicketsSoldOut`    | All tickets have been sold                    | "Sorry, all tickets have been sold!"            |
| 4    | `InsufficientFunds` | User does not have enough balance             | "Insufficient funds to complete this action"    |
| 5    | `NotAuthorized`     | User is not authorized to perform this action | "You are not authorized to perform this action" |

### Prize/Claim Errors (11-20)

| Code | Error                   | Description                         | Frontend Message                         |
| ---- | ----------------------- | ----------------------------------- | ---------------------------------------- |
| 11   | `PrizeNotDeposited`     | Prize has not been deposited yet    | "Prize not yet deposited"                |
| 12   | `PrizeAlreadyClaimed`   | Prize has already been claimed      | "Prize has already been claimed"         |
| 13   | `PrizeAlreadyDeposited` | Prize deposit was already completed | "Prize has already been deposited"       |
| 14   | `NotWinner`             | Only the winner can claim the prize | "You are not the winner of this raffle"  |
| 15   | `ClaimTooEarly`         | Cannot claim before cooldown period | "Please wait before claiming your prize" |

### State/Validation Errors (21-30)

| Code | Error                    | Description                                            | Frontend Message                                         |
| ---- | ------------------------ | ------------------------------------------------------ | -------------------------------------------------------- |
| 21   | `InvalidParameters`      | Invalid input parameters provided                      | "Invalid parameters provided"                            |
| 22   | `InvalidStatus`          | The current raffle status doesn't allow this operation | "This action is not allowed in the current raffle state" |
| 23   | `ContractPaused`         | The contract is paused                                 | "Contract is temporarily paused"                         |
| 24   | `InvalidStateTransition` | Cannot transition to the requested state               | "Cannot change raffle to the requested state"            |
| 25   | `RaffleExpired`          | The raffle end time has passed                         | "This raffle has ended"                                  |

### Ticket Errors (31-40)

| Code | Error                       | Description                         | Frontend Message                               |
| ---- | --------------------------- | ----------------------------------- | ---------------------------------------------- |
| 31   | `InsufficientTickets`       | Not enough tickets sold to finalize | "Minimum ticket requirement not met"           |
| 32   | `MultipleTicketsNotAllowed` | User already has a ticket           | "Multiple tickets not allowed for this raffle" |
| 33   | `NoTicketsSold`             | No tickets have been purchased      | "No tickets have been sold yet"                |
| 34   | `TicketNotFound`            | The requested ticket was not found  | "Ticket not found"                             |

### System Errors (41-60)

| Code | Error                          | Description                                          | Frontend Message                              |
| ---- | ------------------------------ | ---------------------------------------------------- | --------------------------------------------- |
| 41   | `ArithmeticOverflow`           | Arithmetic operation overflow                        | "Calculation error occurred"                  |
| 42   | `AlreadyInitialized`           | Contract is already initialized                      | "Contract already initialized"                |
| 43   | `NotInitialized`               | Contract has not been initialized                    | "Contract not initialized"                    |
| 44   | `Reentrancy`                   | Reentrant call detected                              | "Please try again later"                      |
| 45   | `TokenTransferFailed`          | Token transfer call failed                           | "Payment failed — check your balance"         |
| 46   | `NoActiveTickets`              | All tickets were refunded before the draw            | "No active tickets remain"                    |
| 47   | `DeadlinePassed`               | Swap deadline has passed                             | "Transaction deadline expired"                |
| 48   | `SlippageExceeded`             | Swap output below minimum                            | "Price slippage too high — try again"         |
| 49   | `InvalidIndex`                 | Array index out of bounds                            | "Internal error — invalid index"              |
| 50   | `MorePrizesThanTickets`        | Prize tier count exceeds tickets sold                | "Cannot finalize — more prizes than tickets"  |
| 51   | `ZeroPrize`                    | Calculated prize amount is zero                      | "Prize amount is zero"                        |
| 52   | `InvalidTokenAddress`          | Payment token is not a valid token contract          | "Invalid payment token"                       |
| 53   | `TooManyPrizes`                | Prize tier count exceeds the maximum (100)           | "Too many prize tiers"                        |
| 54   | `EmergencyTooEarly`            | Emergency withdraw time-lock has not elapsed         | "Emergency withdrawal not yet available"      |
| 55   | `InvalidTicketRange`           | `max_tickets` < `min_tickets`                        | "Invalid ticket range configuration"          |
| 56   | `InsufficientAccumulatedFees`  | Withdrawal amount exceeds accumulated fees           | "Not enough fees accumulated"                 |
| 57   | `PrizeConfigurationLocked`     | Prize config cannot be changed after sales begin     | "Prize configuration is locked"               |
| 58   | `OracleRequestAlreadyPending`  | Oracle request already dispatched for this raffle    | "Randomness request already in progress"      |
| 59   | `InvalidStatusForOperation`    | Current status does not allow this operation         | "Operation not allowed in current state"      |
| 60   | `FinalizationAlreadyComplete`  | Raffle has already been finalized                    | "This raffle has already been finalized"      |

> **Note:** Error codes are append-only. Existing codes 1–57 must never be renumbered — on-chain
> storage and off-chain indexers rely on the stable numeric mapping.

---

## Factory Contract Errors

The factory contract (`RaffleFactory`) manages raffle creation. All error codes are defined in the `ContractError` enum in [`contracts/raffle/src/lib.rs`](../contracts/raffle/src/lib.rs).

### General Errors (1-10)

| Code | Error                | Description                    | Frontend Message                |
| ---- | -------------------- | ------------------------------ | ------------------------------- |
| 1    | `AlreadyInitialized` | Factory is already initialized | "Factory already initialized"   |
| 2    | `NotAuthorized`      | User is not the admin          | "You are not the admin"         |
| 3    | `ContractPaused`     | Factory is paused              | "Factory is temporarily paused" |
| 4    | `InvalidParameters`  | Invalid parameters provided    | "Invalid parameters provided"   |
| 5    | `RaffleNotFound`     | Raffle instance not found      | "Raffle not found"              |
| 18   | `TreasuryNotSet`     | Treasury address is not configured | "Treasury address is not set" |

### Admin Errors (11-20)

| Code | Error                  | Description                    | Frontend Message                 |
| ---- | ---------------------- | ------------------------------ | -------------------------------- |
| 11   | `AdminTransferPending` | Admin transfer already pending | "Admin transfer already pending" |
| 12   | `NoPendingTransfer`    | No pending admin transfer      | "No pending admin transfer"      |
| 18   | `UnsupportedSac`       | Payment token is not whitelisted as a supported Stellar Asset Contract | "Unsupported payment token" |

---

## Error Code Mapping

### JavaScript/TypeScript Example

```typescript
// Frontend error mapping
const errorMessages: Record<number, string> = {
  // Instance errors
  1: "Raffle not found",
  2: "This raffle is not currently active",
  3: "Sorry, all tickets have been sold!",
  4: "Insufficient funds to complete this action",
  5: "You are not authorized to perform this action",
  11: "Prize not yet deposited",
  12: "Prize has already been claimed",
  13: "Prize has already been deposited",
  14: "You are not the winner of this raffle",
  15: "Please wait before claiming your prize",
  21: "Invalid parameters provided",
  22: "This action is not allowed in the current raffle state",
  23: "Contract is temporarily paused",
  24: "Cannot change raffle to the requested state",
  25: "This raffle has ended",
  31: "Minimum ticket requirement not met",
  32: "Multiple tickets not allowed for this raffle",
  33: "No tickets have been sold yet",
  34: "Ticket not found",
  41: "Calculation error occurred",
  42: "Contract already initialized",
  43: "Contract not initialized",
  44: "Please try again later",
  45: "Payment failed — check your balance",
  46: "No active tickets remain",
  47: "Transaction deadline expired",
  48: "Price slippage too high — try again",
  49: "Internal error — invalid index",
  50: "Cannot finalize — more prizes than tickets",
  51: "Prize amount is zero",
  52: "Invalid payment token",
  53: "Too many prize tiers",
  54: "Emergency withdrawal not yet available",
  55: "Invalid ticket range configuration",
  56: "Not enough fees accumulated",
  57: "Prize configuration is locked",
  58: "Randomness request already in progress",
  59: "Operation not allowed in current state",
  60: "This raffle has already been finalized",

  // Factory errors
  101: "Factory already initialized",
  102: "You are not the admin",
  103: "Factory is temporarily paused",
  104: "Invalid parameters provided",
  105: "Raffle not found",
  111: "Admin transfer already pending",
  112: "No pending admin transfer",
};

function handleContractError(errorCode: number): string {
  return errorMessages[errorCode] || "An unknown error occurred";
}
```

### React Example

```tsx
import React from "react";

interface ErrorDisplayProps {
  errorCode: number;
}

const ERROR_MESSAGES: Record<number, string> = {
  3: "Sorry, all tickets have been sold!",
  4: "Insufficient funds. Please top up your wallet.",
  14: "You are not the winner of this raffle",
  25: "This raffle has ended",
  // ... add more as needed
};

export const ErrorDisplay: React.FC<ErrorDisplayProps> = ({ errorCode }) => {
  const message =
    ERROR_MESSAGES[errorCode] || "An error occurred. Please try again.";

  return (
    <div className="error-message">
      <span className="error-icon">⚠️</span>
      <span>{message}</span>
    </div>
  );
};
```

---

## Testing Error Handling

All error codes should be tested in the contract test suite to ensure proper error propagation. Run tests with:

```bash
cd contracts/raffle
cargo test
```

---

## Best Practices

1. **Always use Result types**: Never use `panic!()` or `expect()` in production code
2. **Provide meaningful error codes**: Use descriptive error codes that frontend can map to user messages
3. **Document all errors**: Keep this file updated with any new error codes
4. **Handle edge cases**: Test all error paths to ensure proper error propagation
5. **Use appropriate error granularity**: Different errors should have different codes for better UX
