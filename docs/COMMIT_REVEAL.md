# Commit-Reveal Randomness Protocol

This document explains the `RandomnessSource::CommitReveal` protocol, a multi-phase approach to generating fair, verifiable randomness for raffles.

## 1. Protocol Steps

The Commit-Reveal raffle lifecycle consists of four distinct phases:

### 1.1 Raffle Creation

The raffle creator initializes the raffle with `randomness_source = CommitReveal`.

### 1.2 Commit Phase (Active State)

During the raffle's Active state:

- Each ticket buyer generates a local secret: `secret = random_bytes(32)`
- The buyer computes a cryptographic hash: `hash = sha256(secret)`
- The buyer calls `submit_commit(ticket_id, hash)` to commit their hash on-chain

### 1.3 Draw Phase (finalize_raffle)

When `finalize_raffle` is called:

- The contract queries all existing `CommitEntry(ticket_id)` records
- All collected hashes are concatenated and hashed sequentially: `combined = sha256(hash_1 || hash_2 || ... || hash_n)`
- The first 8 bytes of the `combined` hash are extracted and used as the final draw seed

### 1.4 Reveal Phase (Off-Chain, Optional)

After the raffle finalizes, winners can reveal their original secret off-chain to mathematically prove the entropy generation was honest and unmanipulated.

## 2. Ticket Transfer Invariant

Commit entries are **structurally keyed by ticket ID**, not by the owner's public address. This means:

- A commit submitted by the original buyer remains entirely intact and preserved even if the associated ticket is transferred or traded before finalization occurs
- This prevents the silent loss of entropy when tickets change hands

  ## 2.1 One commit per ticket

`submit_commit` stores at most one `CommitEntry` per `ticket_id`.

- A second submission for the same ticket returns `Error::CommitAlreadySubmitted`.
- This blocks last-look bias: a participant cannot overwrite their commitment after
  seeing other on-chain commits and recomputing the prospective seed.

## 2.2 Commit window

Commits are accepted only while the raffle status is **`Active`**.

Once the raffle enters **`Drawing`**, the commit window is closed. Accepting new
entropy after the draw is triggered would give the last committer more information
than earlier participants.

## 3. Fallback Behavior

If zero commits are submitted by the time finalization is triggered, the contract automatically falls back to using an internal PRNG fallback mechanism so the raffle can still be finalized.

## 4. Code Examples

### TypeScript Example

```typescript
import crypto from 'crypto';

function generateCommitHash(): { secret: Buffer; hash: Buffer } {
  const secret = crypto.randomBytes(32);
  const hash = crypto.createHash('sha256').update(secret).digest();
  return { secret, hash };
}

// Usage
const { secret, hash } = generateCommitHash();
// Submit `hash` on-chain via submit_commit(ticket_id, hash)
// Store `secret` securely for potential later reveal
```

### Rust Example

```rust
use sha2::{Sha256, Digest};
use rand::RngCore;

fn generate_commit_hash() -> (Vec<u8>, Vec<u8>) {
    let mut secret = vec![0u8; 32];
    rand::thread_rng().fill_bytes(&mut secret);
    
    let mut hasher = Sha256::new();
    hasher.update(&secret);
    let hash = hasher.finalize().to_vec();
    
    (secret, hash)
}

// Usage
let (secret, hash) = generate_commit_hash();
// Submit `hash` on-chain via submit_commit(ticket_id, hash)
// Store `secret` securely for potential later reveal
```
