# #737 — `Ticket::new` does not set `payer`
Status in current `master`: already addressed — `contracts/raffle-shared/src/lib.rs` `Ticket::new` now sets `payer: owner` (the struct declares it and the constructor initialises it).

# #761 — `buy_tickets_for` dup of `buy_tickets`, not exposed in API
Scoped refactor (`contracts/raffle-instance/src/tickets.rs`): de-duplicate `buy_tickets_for` onto `buy_tickets` and expose it in the contract API. Implementation + tests to follow in the hardening pass.
