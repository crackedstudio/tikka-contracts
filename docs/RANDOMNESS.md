# Randomness Protocol Design & Trust Model

This document outlines the cryptographic design, trust model, and security considerations of the Raffle's randomness protocol, focusing on the Verifiable Random Function (VRF) mode and the multi-operator `k-of-n` Quorum mode.

---

## 1. Single-Oracle VRF Mode

In the single-oracle mode, a single trusted oracle is responsible for generating and delivering randomness.

### Protocol Flow
1. The Raffle contract emits a `RandomnessRequested` event containing a unique `request_id`.
2. The Oracle service detects the event, reads the `request_id` and contract ID, and generates a Verifiable Random Function (VRF) proof.
3. The Oracle submits the proof and the generated random seed back to the contract via `provide_randomness`.
4. The contract verifies the VRF proof on-chain using the Oracle's public key. If the proof is valid, the random seed is accepted.

### Trust Model & Mitigations
- **Unpredictability**: Because VRF proofs are cryptographically tied to the Oracle's private key, the random seed is completely unpredictable to anyone (including the players) before it is submitted.
- **Non-manipulability**: The Oracle cannot bias the randomness because there is only one valid VRF output for a given input (`request_id` + contract address). The Oracle's only options are to submit the correct value or refuse to submit (causing a Denial of Service, which is monitored and alerted).

---

## 2. Multi-Operator Quorum Mode (k-of-n)

In Quorum mode, a decentralized group of $n$ independent oracles participate, and at least $k$ unique oracle submissions are required to construct the final random seed.

### Protocol Flow
1. The Raffle contract is configured with quorum parameters $k$ (threshold) and a list of $n$ authorized oracle addresses.
2. The contract emits a `RandomnessRequested` event.
3. Each participating oracle generates a cryptographically secure random seed independently and submits it to the contract via `provide_quorum_randomness(request_id, random_seed)`.
4. The contract stores the submitted seeds.
5. Once $k$ unique oracles have submitted their seeds, the contract combines the seeds (typically by hashing them together, e.g., `hash(seed_1 + seed_2 + ... + seed_k)`) to produce the final raffle seed.

---

## 3. Last-Submitter Bias

The primary security challenge in threshold-based randomness protocols is **Last-Submitter Bias** (or Last-Revealer Bias).

### The Attack Vector
When $k-1$ oracles have submitted their seeds on-chain, those seeds are public. The $k$-th oracle (the last submitter required to reach the threshold) can:
1. Read the $k-1$ public seeds.
2. Precompute the combined raffle seed for different values of their own seed, or simply calculate the single outcome of their submission.
3. Determine the winning ticket based on that combined seed.
4. **Bias the outcome**: If the $k$-th oracle (or a colluding party) is unhappy with the winner (e.g., they didn't win), they can choose to **withhold** their submission, refusing to complete the quorum. They might wait for the raffle to time out (allowing refunds) or wait for a different block height if the contract allows late submissions under different conditions.

### Mitigations in Tikka Contracts

#### 1. Independent Cryptographic Seeds
No single oracle can force the final seed to be a specific desired value. Because the final seed is a hash of all $k$ seeds, changing the $k$-th seed changes the final hash in an unpredictable way (due to the avalanche effect of cryptographic hash functions). The last submitter can only choose between two options:
- Submit their honest seed and accept the resulting winner.
- Abort/withhold the transaction, preventing the draw from finishing.

#### 2. Decentralized & Reputation-Bound Operators
Oracles are run by reputable, independent node operators. Collusion between $k$ operators is required to predict or manipulate the final seed. The threshold $k$ should be chosen such that $k > n/2$ (a simple majority) to ensure that a minority of colluding operators cannot reconstruct or manipulate the randomness.

#### 3. Timeouts and Default Fallbacks
To prevent a malicious or lazy $k$-th oracle from holding the raffle hostage indefinitely, the contract implements:
- **Draw Timeouts**: If a quorum is not reached within a specified block window, the raffle can be cancelled, and all ticket buyers are refunded.
- **Slashed Stake / Operator Penalties**: Node operators who fail to submit within the timeout window can be penalized on-chain or removed from the active oracle set.
