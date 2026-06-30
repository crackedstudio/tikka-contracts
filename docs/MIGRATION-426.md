# Migration Guide — #426: Stable-Index Raffle Storage

## What changed

The factory contract (`raffle-factory`) no longer stores raffle addresses in a
single persistent `Vec` under `DataKey::RaffleInstances`.  That pattern caused
every `create_raffle` and `clean_old_raffle` call to deserialise and reserialise
the entire list, which grows without bound as more raffles are created.

The new layout uses three independent storage keys:

| Key | Type | Purpose |
|-----|------|---------|
| `DataKey::RaffleById(u32)` | `Address` | Stable map: stable_id → raffle address |
| `DataKey::NextRaffleId` | `u32` | Monotonic counter; next ID to assign |
| `DataKey::RaffleCount` | `u32` | Live (non-tombstoned) raffle count |

`TotalRafflesCreated` (cumulative, never decremented) is unchanged.

### Complexity comparison

| Operation | Before | After |
|-----------|--------|-------|
| `create_raffle` | O(n) — deserialise full Vec | **O(1)** — single slot write |
| `clean_old_raffle` | O(n) — deserialise + swap-remove | **O(1)** — slot delete |
| `get_raffles_page(limit)` | O(n) — deserialise full Vec | **O(limit)** — range reads |
| `get_raffle_by_id(id)` | O(n) — linear scan | **O(1)** — direct slot read |

### Stable IDs

- IDs start at 0 and are never reused.
- Removing a raffle via `clean_old_raffle(id)` **tombstones** that slot
  (removes the entry).  All other IDs are unaffected — no shifting,
  no reindexing.
- `get_raffles_page` silently skips tombstoned slots.
- `get_raffle_by_id` returns `None` for tombstoned or unassigned IDs.

### New public entry points

```rust
/// O(1) direct lookup.
get_raffle_by_id(env: Env, raffle_id: u32) -> Option<Address>

/// Next ID to be assigned (== total raffles ever created).
get_next_raffle_id(env: Env) -> u32

/// Live (non-tombstoned) raffle count.
get_raffle_count(env: Env) -> u32
```

### Removed / renamed

- `DataKey::RaffleInstances` — removed entirely.
- `require_registered_raffle` internal helper — removed (#412; was dead code that incorrectly called `raffle_address.require_auth()`).
- `init_factory` no longer seeds an empty Vec.

---

## Testnet migration

The current testnet deployment
(`CCTCPMI66REXIJQPVOPNTNUZBCMSRM7TZLMIPQROZIID44XNP2P2MKFZ`,
 deployed 2026-02-24) was deployed before this change.  It holds the old
`RaffleInstances` Vec in persistent storage.

### Steps

1. **Upgrade the factory WASM** via `upgrade(new_wasm_hash)` (requires the
   48-hour timelock via `set_config` → `execute_config_change`).

2. **Run the one-time migration script** below.  It reads the old Vec,
   writes each address into the new `RaffleById(i)` slot, sets
   `NextRaffleId` to `Vec.len()`, sets `RaffleCount` to `Vec.len()`, and
   finally removes `RaffleInstances`.

   ```typescript
   // oracle/src/migrate-426.ts  (illustrative — adapt to your SDK version)
   import * as StellarSdk from "@stellar/stellar-sdk";

   const FACTORY = "CCTCPMI66REXIJQPVOPNTNUZBCMSRM7TZLMIPQROZIID44XNP2P2MKFZ";

   async function migrate() {
     const server = new StellarSdk.SorobanRpc.Server(
       "https://soroban-testnet.stellar.org"
     );

     // 1. Read the old Vec from ledger storage.
     const key = xdr.ScVal.scvLedgerKeyContractData({
       contract: StellarSdk.Address.fromString(FACTORY).toScAddress(),
       key: /* xdr encode DataKey::RaffleInstances */,
       durability: xdr.ContractDataDurability.persistent(),
     });
     const entries = await server.getLedgerEntries(key);
     const vec: string[] = /* decode entries[0].val as Vec<Address> */;

     // 2. For each address, write DataKey::RaffleById(i) via an admin tx.
     //    (Use the factory's admin keypair; batch into one tx per ledger limit.)
     for (let i = 0; i < vec.length; i++) {
       await adminInvoke("write_raffle_by_id", [i, vec[i]]); // hypothetical
     }

     // 3. Write NextRaffleId and RaffleCount.
     await adminInvoke("set_next_raffle_id", [vec.length]);
     await adminInvoke("set_raffle_count",   [vec.length]);

     // 4. Remove the old key (will naturally expire if not extended, or
     //    can be cleared via a dedicated admin function if added).
   }
   ```

   > **Note:** The factory contract does not currently expose a public
   > `write_raffle_by_id` entry point.  For a production migration the
   > recommended approach is to add a one-shot, admin-only `migrate_v426()`
   > function to the contract that reads the old Vec and writes the new
   > keys in a single transaction, then removes itself from the WASM
   > (or is simply never callable again via a `migrated` flag).

3. **Verify** by calling `get_raffle_by_id(0)` and comparing with the
   first element of the old Vec, and `get_next_raffle_id()` matches
   `Vec.len()`.

4. **Off-chain indexers / clients** must be updated:
   - Replace any code that used the full `get_raffles_page` Vec offset as a
     stable raffle identifier with the new `stable_id` (`u32`) from
     `NextRaffleId - 1` at creation time.
   - The `RaffleCleanedUp` event's `raffle_address` field is unchanged; but
     the numeric `raffle_id` argument to `clean_old_raffle` now refers to a
     stable ID, not a Vec index.

### Mainnet

There is no recorded mainnet deployment.  For a future mainnet deployment
simply deploy the new WASM; no migration is needed for a fresh factory.
