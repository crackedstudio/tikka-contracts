# Tikka Protocol Fee Model

## Fee Collection & Accounting Model (Pull Model)

Tikka smart contracts use a **Pull Model** for protocol fee collection:
1. **Accrual at Collection Time:** Protocol fees collected during ticket purchases or prize claims are accumulated in instance storage under `DataKey::AccumulatedFees`. Fees are **not** immediately transferred to the treasury at purchase/claim time.
2. **Pull Withdrawal:** The admin pulls accrued protocol fees to a designated recipient/treasury using `withdraw_fees` once the raffle is `Finalized` or `Claimed`.

### At Ticket Purchase
- **Formula:** `(total_price × protocol_fee_bp) / 10000`
- **Accounting:** Added to `AccumulatedFees` in contract storage.
- **Payer:** Ticket buyer (fee is included in total ticket cost).

### Tier Prize Allocation
- **Formula:** `prize_amount × tier_basis_points / 10000` for every tier except the final tier.
- **Final tier:** Receives `prize_amount` minus the amounts allocated to all earlier tiers.
- **Rounding Rule:** Integer-division dust is assigned to the final tier, so all tier prizes sum exactly to `prize_amount` and no prize funds remain undistributed.

## Effective Total Fee

For a raffle with protocol_fee_bp = 250 (2.5%), ticket_price = 100 XLM, and 10 tickets:

- Ticket fees accrued: 10 × 2.5 XLM = 25 XLM (stored in `AccumulatedFees`)
- Prize claim fee: 0 XLM
- **Total protocol revenue available for `withdraw_fees`: 25 XLM**

