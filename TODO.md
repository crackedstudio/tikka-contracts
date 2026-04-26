# Fix Plan - Tikka Contracts Compilation & Logic Errors

## Steps

- [ ] 1. Fix `ProtocolStats` struct in `lib.rs` (remove invalid enum-variant lines)
- [ ] 2. Fix `RaffleStatus` enum in `instance/mod.rs` (add missing `Active` and `Claimed` variants)
- [ ] 3. Fix `deposit_prize` to transition `Open → Active`
- [ ] 4. Fix `finalize_raffle` (remove dead code, fix variable shadowing)
- [ ] 5. Fix `provide_randomness` (replace undefined `do_finalize_with_seed` call)
- [ ] 6. Fix `claim_prize` (define `old_status`, transition to `Claimed` when all prizes claimed)
- [ ] 7. Fix `trigger_randomness_fallback` (undefined `winner_ticket` reference)
- [ ] 8. Update `docs/EVENTS.md` status documentation
- [ ] 9. Verify compilation
