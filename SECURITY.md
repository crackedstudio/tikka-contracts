# Security Overview

## Front-Running Mitigation: Randomness Fulfillment Delay

### Attack Vector
The raffle contract's randomness fulfillment mechanism (`provide_randomness`) was vulnerable to front-running and manipulation attacks. In the original implementation, an oracle could submit randomness immediately after a raffle transitioned to `Drawing`, potentially allowing an attacker to:
1. Observe the pending raffle finalization
2. Manipulate oracle behavior to favor specific outcomes
3. Execute malicious transactions in the same block

### Mitigation Implemented
To address this vulnerability, we've implemented a minimum ledger delay between randomness request and fulfillment:

- A constant `RANDOMNESS_MIN_DELAY_LEDGERS = 10` is enforced
- When randomness is requested (during the Drawing phase transition), the current ledger sequence is stored under `DataKey::RandomnessRequestLedger`
- In `provide_randomness`, we check that the current ledger sequence is at least 10 ledgers higher than the request ledger
- If fulfillment is attempted too early, the transaction is rejected with `Error::RandomnessTooEarly`

This delay ensures there's sufficient time for:
- The market and participants to stabilize
- No same-block manipulation
- A clear window between request and fulfillment

### Other Security Considerations
- **Drawing Lock**: Exclusive lock to prevent concurrent state transitions
- **Oracle Timeout**: Fallback mechanism if oracle doesn't respond within 200 ledgers
- **Reentrancy Guard**: Prevents reentrant attacks
