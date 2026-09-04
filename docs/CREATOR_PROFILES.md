# Creator Profiles

> **Implementation note:** This feature is now implemented in `contracts/raffle-factory/src/registry.rs`. The registry module holds no funds and gates no funds movement.

## Overview

Creator profiles provide lightweight on-chain identity and reputation for raffle organizers. Buyers can distinguish reputable organizers from throwaway addresses without requiring off-chain infrastructure.

## Profile Structure

Each creator profile contains:

```rust
pub struct CreatorProfile {
    pub name: String,           // Display name (max 1000 bytes)
    pub verified: bool,         // Admin-granted trust badge
    pub raffles_created: u32,   // Automatic track record counter
}
```

## Features

### Display Name

- **Self-Service**: Creators set their own display name via `set_profile_name`
- **Length Limit**: Maximum 1000 bytes (same as raffle descriptions)
- **No Uniqueness**: Multiple creators can use the same name
- **Mutable**: Can be updated anytime

### Verified Badge

- **Admin-Controlled**: Only factory admin can grant/revoke verified status
- **Trust Signal**: Indicates organizer has been vetted or is a known entity
- **Binary Flag**: Either verified (`true`) or not (`false`)

### Track Record

- **Automatic**: `raffles_created` increments on every successful `create_raffle`
- **Immutable**: Cannot be manually adjusted, only incremented by contract
- **Lifetime Count**: Total raffles ever created by this address

## Usage

### Setting Profile Name

Creators can set their display name at any time:

```rust
factory.set_profile_name(
    &creator_address,
    &String::from_str(&env, "Alice's Premium Raffles")
);
```

**Requirements:**
- Caller must be the profile owner (authorization required)
- Name must not exceed 1000 bytes

**Events:** Emits `ProfileNameSet`

### Admin Verification

Factory admin can grant verified status:

```rust
factory.set_verified(
    &creator_address,
    &true  // true = verified, false = unverified
);
```

**Requirements:**
- Caller must be factory admin

**Events:** Emits `VerifiedStatusSet`

### Querying Profiles

Anyone can read a profile (no authorization required):

```rust
let profile = factory.get_profile(&creator_address);

// Returns CreatorProfile with:
// - name: display name (empty string if never set)
// - verified: true if admin-verified, false otherwise
// - raffles_created: number of raffles created
```

**Default Values:**
- New/unknown addresses return a default profile:
  - `name`: empty string
  - `verified`: false
  - `raffles_created`: 0

## Frontend Integration

### Display Pattern

```
┌────────────────────────────────────┐
│ Alice's Premium Raffles ✓          │
│ 47 raffles created                 │
└────────────────────────────────────┘
```

- Show display name prominently
- Display verified badge (✓, checkmark, badge icon) if `verified == true`
- Show `raffles_created` as social proof

### Trust Indicators

Frontends can build trust signals:

```
High Trust:    verified = true, raffles_created > 50
Medium Trust:  verified = false, raffles_created > 20
New Creator:   verified = false, raffles_created < 5
```

### Sorting & Filtering

- Sort creators by `raffles_created` (most experienced first)
- Filter to show only verified creators
- Group raffles by creator using existing `get_raffles_by_creator`

## Storage Costs

Each profile occupies one persistent storage entry:

- **Key**: `DataKey::CreatorProfile(Address)`
- **Value**: `CreatorProfile` struct (~1KB max with full name)
- **Lifetime**: Persistent (survives contract upgrades)

Profiles are created on-demand:
- First `set_profile_name`, `set_verified`, or `create_raffle` triggers creation
- Unused addresses consume zero storage

## Security Considerations

### Display Name

- **No Validation**: Names are not checked for duplicates, profanity, or impersonation
- **Frontend Responsibility**: UIs should sanitize display
- **Length Cap**: Prevents storage abuse (1000 byte limit)

### Verified Badge

- **Single Source of Truth**: Only factory admin controls verified status
- **No Automation**: Admin must manually review and verify each creator
- **Revocable**: Admin can remove verification at any time

### Track Record

- **Tamper-Proof**: Count increments automatically, cannot be manipulated
- **Lifetime Metric**: Never decrements (even if raffles are cleaned up)
- **No Quality Signal**: High count doesn't guarantee good outcomes

## Events

### ProfileNameSet

```rust
pub struct ProfileNameSet {
    pub creator: Address,
    pub name: String,
    pub timestamp: u64,
}
```

**Topics:** `("tikka", "profile_name_set")`

### VerifiedStatusSet

```rust
pub struct VerifiedStatusSet {
    pub creator: Address,
    pub verified: bool,
    pub set_by: Address,    // admin who made the change
    pub timestamp: u64,
}
```

**Topics:** `("tikka", "verified_status_set")`

## API Reference

### set_profile_name

```rust
pub fn set_profile_name(
    env: Env,
    creator: Address,
    name: String,
) -> Result<(), ContractError>
```

**Authorization:** Requires `creator` signature  
**Errors:**
- `InvalidParameters` — name exceeds 1000 bytes

### set_verified

```rust
pub fn set_verified(
    env: Env,
    creator: Address,
    verified: bool,
) -> Result<(), ContractError>
```

**Authorization:** Requires admin signature  
**Errors:**
- `NotAuthorized` — caller is not admin

### get_profile

```rust
pub fn get_profile(
    env: Env,
    creator: Address,
) -> CreatorProfile
```

**Authorization:** None (public view)  
**Returns:** Profile with defaults if not found

## Migration Notes

Existing raffles created before this feature:
- Creators will have `raffles_created = 0` initially
- Historical count is not backfilled
- Profile only increments on new raffles going forward

## Future Enhancements

Potential extensions (not currently implemented):

- **Social Links**: Twitter, Discord, website URLs
- **Profile Images**: IPFS hash for avatar/banner
- **Statistics**: Win rate, claim rate, refund rate
- **Reputation Scores**: Derived from buyer feedback
- **Name Registry**: Optional unique name reservation system
