#![no_std]
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, xdr::ToXdr, Address, Bytes, BytesN, Env,
    contract, contracterror, contractimpl, contracttype, token, Address, Bytes, BytesN, Env,
    IntoVal, Symbol, Vec,
};

#[cfg(not(test))]
use soroban_sdk::xdr::ToXdr;

#[cfg(test)]
use soroban_sdk::testutils::Address as _;

mod events;

use raffle_shared::{
    effective_limit, AdminOp, FairnessData, PageResultRaffles, PaginationParams, RaffleConfig,
};

use raffle_shared::constants::{CHECKPOINT_INTERVAL, MAX_PROTOCOL_FEE_BP, TIMELOCK_DELAY_SECONDS};

#[derive(Clone)]
#[contracttype]
pub struct PendingOp {
    pub op: AdminOp,
    pub effective_timestamp: u64,
    pub proposed_by: Address,
}

#[derive(Clone)]
#[contracttype]
pub struct StateCheckpoint {
    pub index: u32,
    pub raffle_count: u32,
    pub ledger_timestamp: u64,
    pub aggregate_hash: BytesN<32>,
}

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Initialized,
    Admin,
    /// Stable map: stable_id (u32) → raffle Address.
    /// Replaces the old RaffleInstances Vec — each entry is an independent
    /// storage slot so reads and writes are always O(1).
    RaffleById(u32),
    /// Monotonic counter: the stable_id that will be assigned to the *next*
    /// raffle.  Starts at 0 and is never decremented.
    NextRaffleId,
    /// Number of live (non-tombstoned) raffles.  Used for stats only.
    RaffleCount,
    InstanceWasmHash,
    ProtocolFeeBP,
    Treasury,
    Paused,
    PendingAdmin,
    PendingOp(u32),
    OpCounter,
    Checkpoint(u32),
    LatestCheckpointIndex,
    TotalRafflesCreated,
    UniqueParticipant(Address),
    TotalUniqueParticipants,
    MinCreationDelay,
    LastCreationTime(Address),
    WhitelistedPartner(Address),
    TotalVolumePerAsset(Address),
    /// Kept for test-only address generation; not used for indexing.
    RaffleInstancesCount,
    /// Per-creator raffle index: creator Address → Vec<Address> of raffle addresses.
    /// Appended to on every successful `create_raffle`.
    CreatorRaffles(Address),
}

#[derive(Clone)]
#[contracttype]
pub struct ProtocolStats {
    pub total_raffles_created: u32,
    pub protocol_fee_bp: u32,
    pub paused: bool,
    pub total_unique_participants: u32,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum ContractError {
    AlreadyInitialized = 1,
    NotAuthorized = 2,
    ContractPaused = 3,
    InvalidParameters = 4,
    RaffleNotFound = 5,
    AdminTransferPending = 11,
    NoPendingTransfer = 12,
    RateLimitExceeded = 13,
    NoPendingOp = 14,
    TimelockNotElapsed = 15,
    InvalidRaffleId = 16,
    RaffleNotEligible = 17,
    ArithmeticOverflow = 18,
    TreasuryNotSet = 19,
}

#[contract]
pub struct RaffleFactory;

raffle_shared::impl_require_admin!(ContractError, ContractError::NotAuthorized);
raffle_shared::impl_require_not_paused!(ContractError, ContractError::ContractPaused, require_factory_not_paused);

fn maybe_create_checkpoint(env: &Env, raffle_count: u32) {
    if raffle_count == 0 || !raffle_count.is_multiple_of(CHECKPOINT_INTERVAL) {
        return;
    }

    let index = raffle_count / CHECKPOINT_INTERVAL;
    let ledger_timestamp = env.ledger().timestamp();
    let ledger_sequence = env.ledger().sequence();

    let mut input = Bytes::new(env);
    input.extend_from_array(&raffle_count.to_be_bytes());
    input.extend_from_array(&ledger_sequence.to_be_bytes());
    input.extend_from_array(&ledger_timestamp.to_be_bytes());

    let aggregate_hash = env.crypto().sha256(&input);

    let checkpoint = StateCheckpoint {
        index,
        raffle_count,
        ledger_timestamp,
        aggregate_hash: aggregate_hash.clone().into(),
    };

    env.storage()
        .persistent()
        .set(&DataKey::Checkpoint(index), &checkpoint);
    env.storage()
        .persistent()
        .set(&DataKey::LatestCheckpointIndex, &index);

    events::CheckpointCreated {
        index,
        raffle_count,
        ledger_timestamp,
        aggregate_hash: aggregate_hash.into(),
    }
    .publish(&env);
    .publish(env);
}

/// Validate that an address is usable for a privileged role (admin/treasury).
///
/// Rejects the zero contract address (all-zero 32-byte hash) and the factory's
/// own address to prevent a self-referential admin or treasury that would brick
/// the contract.  Account (keypair) addresses are always accepted.
fn require_valid_role_address(env: &Env, address: &Address) -> Result<(), ContractError> {
    #[cfg(not(test))]
    if !address.exists() {
        return Err(ContractError::InvalidParameters);
    }
    // In test mode the exists() check is skipped, but we still reject the
    // all-zeros contract id (the "zero address") explicitly.
    #[cfg(test)]
    {
        use soroban_sdk::String;
        const ZERO_CONTRACT: &str = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4";
        let zero = Address::from_string(&String::from_str(env, ZERO_CONTRACT));
        if *address == zero {
            return Err(ContractError::InvalidParameters);
        }
    }
    if *address == env.current_contract_address() {
        return Err(ContractError::InvalidParameters);
    }
    Ok(())
}

#[contractimpl]
impl RaffleFactory {
    pub fn init_factory(
        env: Env,
        admin: Address,
        wasm_hash: BytesN<32>,
        protocol_fee_bp: u32,
        treasury: Address,
    ) -> Result<(), ContractError> {
        if env.storage().persistent().has(&DataKey::Initialized) {
            return Err(ContractError::AlreadyInitialized);
        }
        if protocol_fee_bp > MAX_PROTOCOL_FEE_BP {
            return Err(ContractError::InvalidParameters);
        }
        require_valid_role_address(&env, &admin)?;
        require_valid_role_address(&env, &treasury)?;
        env.storage().persistent().set(&DataKey::Admin, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::InstanceWasmHash, &wasm_hash);
        env.storage()
            .persistent()
            .set(&DataKey::ProtocolFeeBP, &protocol_fee_bp);
        env.storage()
            .persistent()
            .set(&DataKey::Treasury, &treasury);
        env.storage().persistent().set(&DataKey::Initialized, &true);

        events::FactoryInitialized {
            admin,
            protocol_fee_bp,
            treasury,
            timestamp: env.ledger().timestamp(),
        }
        .publish(&env);

        Ok(())
    }

    pub fn set_config(
        env: Env,
        protocol_fee_bp: u32,
        treasury: Address,
    ) -> Result<u32, ContractError> {
        let admin = require_admin(&env)?;
        if protocol_fee_bp > MAX_PROTOCOL_FEE_BP {
            return Err(ContractError::InvalidParameters);
        }
        require_valid_role_address(&env, &treasury)?;

        let op_id = env
            .storage()
            .persistent()
            .get::<_, u32>(&DataKey::OpCounter)
            .unwrap_or(0)
            .saturating_add(1);

        env.storage().persistent().set(&DataKey::OpCounter, &op_id);

        let effective_timestamp = env.ledger().timestamp() + TIMELOCK_DELAY_SECONDS;
        let op = AdminOp::SetConfig(protocol_fee_bp, treasury.clone());
        let pending = PendingOp {
            op: op.clone(),
            effective_timestamp,
            proposed_by: admin.clone(),
        };
        env.storage()
            .persistent()
            .set(&DataKey::PendingOp(op_id), &pending);

        events::AdminOpProposed {
            op_id,
            op,
            effective_timestamp,
            proposed_by: admin,
        }
        .publish(&env);

        Ok(op_id)
    }

    pub fn execute_config_change(env: Env, op_id: u32) -> Result<(), ContractError> {
        let admin = require_admin(&env)?;

        let pending: PendingOp = env
            .storage()
            .persistent()
            .get(&DataKey::PendingOp(op_id))
            .ok_or(ContractError::NoPendingOp)?;

        if env.ledger().timestamp() < pending.effective_timestamp {
            return Err(ContractError::TimelockNotElapsed);
        }

        match pending.op.clone() {
            AdminOp::SetConfig(protocol_fee_bp, treasury) => {
                if protocol_fee_bp > MAX_PROTOCOL_FEE_BP {
                    return Err(ContractError::InvalidParameters);
                }
                require_valid_role_address(&env, &treasury)?;
                env.storage()
                    .persistent()
                    .set(&DataKey::ProtocolFeeBP, &protocol_fee_bp);
                env.storage()
                    .persistent()
                    .set(&DataKey::Treasury, &treasury);
            }
            AdminOp::UpdateWasmHash(new_hash) => {
                env.storage()
                    .persistent()
                    .set(&DataKey::InstanceWasmHash, &new_hash);
            }
        }

        env.storage()
            .persistent()
            .remove(&DataKey::PendingOp(op_id));

        events::AdminOpExecuted {
            op_id,
            op: pending.op,
            executed_by: admin,
            executed_at: env.ledger().timestamp(),
        }
        .publish(&env);

        Ok(())
    }

    pub fn cancel_config_change(env: Env, op_id: u32) -> Result<(), ContractError> {
        let admin = require_admin(&env)?;

        if !env.storage().persistent().has(&DataKey::PendingOp(op_id)) {
            return Err(ContractError::NoPendingOp);
        }

        env.storage()
            .persistent()
            .remove(&DataKey::PendingOp(op_id));

        events::AdminOpCancelled {
            op_id,
            cancelled_by: admin,
            cancelled_at: env.ledger().timestamp(),
        }
        .publish(&env);

        Ok(())
    }

    pub fn get_pending_op(env: Env, op_id: u32) -> Option<PendingOp> {
        env.storage().persistent().get(&DataKey::PendingOp(op_id))
    }

    pub fn get_op_counter(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::OpCounter)
            .unwrap_or(0u32)
    }

    pub fn create_raffle(
        env: Env,
        creator: Address,
        config: RaffleConfig,
    ) -> Result<Address, ContractError> {
        creator.require_auth();
        require_factory_not_paused(&env)?;

        let is_whitelisted = env
            .storage()
            .persistent()
            .get(&DataKey::WhitelistedPartner(creator.clone()))
            .unwrap_or(false);

        if !is_whitelisted {
            let now = env.ledger().timestamp();
            let min_delay = env
                .storage()
                .persistent()
                .get(&DataKey::MinCreationDelay)
                .unwrap_or(300);

            let last_creation: u64 = env
                .storage()
                .persistent()
                .get(&DataKey::LastCreationTime(creator.clone()))
                .unwrap_or(0);

            if now < last_creation + min_delay {
                let unlock_timestamp = last_creation + min_delay;
                events::CreationRateLimited {
                    creator: creator.clone(),
                    unlock_timestamp,
                    timestamp: now,
                }
                .publish(&env);
                return Err(ContractError::RateLimitExceeded);
            }

            env.storage()
                .persistent()
                .set(&DataKey::LastCreationTime(creator.clone()), &now);
        }

        let protocol_fee_bp: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::ProtocolFeeBP)
            .unwrap_or(0);
        let treasury: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Treasury)
            .ok_or(ContractError::TreasuryNotSet)?;

        let mut final_config = config;
        final_config.protocol_fee_bp = protocol_fee_bp;
        final_config.treasury_address = Some(treasury);

        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .ok_or(ContractError::NotAuthorized)?;
        let factory_address = env.current_contract_address();

        let salt = env
            .crypto()
            .sha256(&(creator.clone(), final_config.description.clone()).to_xdr(&env));

        #[cfg(not(test))]
        let raffle_address = {
            let wasm_hash: BytesN<32> = env
                .storage()
                .persistent()
                .get(&DataKey::InstanceWasmHash)
                .ok_or(ContractError::InvalidParameters)?;
            let salt = env
                .crypto()
                .sha256(&(creator.clone(), final_config.description.clone()).to_xdr(&env));
            env.deployer()
                .with_address(factory_address.clone(), salt)
                .deploy_v2(wasm_hash, ())
        };

        #[cfg(test)]
        let raffle_address = {
            let mut count: u32 = env
                .storage()
                .persistent()
                .get(&DataKey::RaffleInstancesCount)
                .unwrap_or(0);
            count += 1;
            env.storage()
                .persistent()
                .set(&DataKey::RaffleInstancesCount, &count);

            let mut id = Address::generate(&env);
            for _ in 0..count {
                id = Address::generate(&env);
            }
            env.register_at(&id, raffle_instance::Contract, ());
            id
        };

        env.invoke_contract::<()>(
            &raffle_address,
            &Symbol::new(&env, "init"),
            (factory_address, admin, creator.clone(), final_config).into_val(&env),
        );

        // --- O(1) stable-map registration ---
        // Assign the next stable ID and write a single entry.  No Vec is
        // deserialised or reserialised; each raffle occupies its own storage
        // slot, so cost is constant regardless of how many raffles exist.
        let stable_id: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::NextRaffleId)
            .unwrap_or(0u32);
        env.storage()
            .persistent()
            .set(&DataKey::RaffleById(stable_id), &raffle_address);
        env.storage()
            .persistent()
            .set(&DataKey::NextRaffleId, &(stable_id.saturating_add(1)));

        // --- per-creator index ---
        // Append the new raffle address to the creator's list so callers can
        // query all raffles for a given creator without scanning the full list.
        let mut creator_raffles: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::CreatorRaffles(creator.clone()))
            .unwrap_or_else(|| Vec::new(&env));
        creator_raffles.push_back(raffle_address.clone());
        env.storage()
            .persistent()
            .set(&DataKey::CreatorRaffles(creator.clone()), &creator_raffles);

        // Increment the live-count for stats.
        let live_count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::RaffleCount)
            .unwrap_or(0u32)
            .saturating_add(1);
        env.storage()
            .persistent()
            .set(&DataKey::RaffleCount, &live_count);

        let mut count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalRafflesCreated)
            .unwrap_or(0);
        count += 1;
        env.storage()
            .persistent()
            .set(&DataKey::TotalRafflesCreated, &count);

        maybe_create_checkpoint(&env, count);

        Ok(raffle_address)
    }

    pub fn get_protocol_stats(env: Env) -> ProtocolStats {
        let total_raffles_created: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalRafflesCreated)
            .unwrap_or(0);
        let protocol_fee_bp: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::ProtocolFeeBP)
            .unwrap_or(0);
        let paused: bool = env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false);
        let total_unique_participants: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalUniqueParticipants)
            .unwrap_or(0);

        ProtocolStats {
            total_raffles_created,
            protocol_fee_bp,
            paused,
            total_unique_participants,
        }
    }

    /// O(1) direct lookup of a raffle address by its stable ID.
    /// Returns `None` if the ID was never assigned or has been cleaned up.
    pub fn get_raffle_by_id(env: Env, raffle_id: u32) -> Option<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::RaffleById(raffle_id))
    }

    /// Returns the stable ID that will be assigned to the next raffle.
    /// IDs in [0, next_raffle_id) have been assigned at least once.
    pub fn get_next_raffle_id(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::NextRaffleId)
            .unwrap_or(0u32)
    }

    /// Returns the current count of live (non-tombstoned) raffles.
    pub fn get_raffle_count(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::RaffleCount)
            .unwrap_or(0u32)
    }

    pub fn get_total_volume(env: Env, asset: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::TotalVolumePerAsset(asset))
            .unwrap_or(0)
    }

    pub fn record_volume(env: Env, asset: Address, amount: i128) -> Result<(), ContractError> {
        let total_volume: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalVolumePerAsset(asset.clone()))
            .unwrap_or(0);
        let total_volume = total_volume
            .checked_add(amount)
            .ok_or(ContractError::ArithmeticOverflow)?;
        env.storage()
            .persistent()
            .set(&DataKey::TotalVolumePerAsset(asset), &total_volume);
        Ok(())
    }

    pub fn get_admin(env: Env) -> Result<Address, ContractError> {
        env.storage()
            .persistent()
            .get(&DataKey::Admin)
            .ok_or(ContractError::NotAuthorized)
    }

    pub fn get_raffles_page(env: Env, params: PaginationParams) -> PageResultRaffles {
        // `NextRaffleId` is the exclusive upper bound on all ever-assigned IDs.
        // It equals the total number of raffles ever created (including any that
        // have been cleaned up / tombstoned).
        let next_id: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::NextRaffleId)
            .unwrap_or(0u32);

        let lim = effective_limit(params.limit);
        let offset = params.offset;

        // `total` here is the live-raffle count (tombstoned entries excluded),
        // reported to the caller for UI pagination purposes.
        let total: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::RaffleCount)
            .unwrap_or(0u32);

        if offset >= next_id {
            return PageResultRaffles {
                items: Vec::new(&env),
                total,
                has_more: false,
            };
        }

        // Walk the stable ID space [offset, offset + lim) and collect only
        // slots that still hold a live address (non-tombstoned).  Each read
        // is a single O(1) storage lookup; the loop is bounded by `lim`.
        let end = offset.saturating_add(lim).min(next_id);
        let mut items: Vec<Address> = Vec::new(&env);
        for id in offset..end {
            if let Some(addr) = env
                .storage()
                .persistent()
                .get::<_, Address>(&DataKey::RaffleById(id))
            {
                items.push_back(addr);
            }
        }

        let has_more = end < next_id;
        PageResultRaffles {
            items,
            total,
            has_more,
        }
    }

    /// Return a paginated list of raffle addresses created by `creator`.
    ///
    /// `params.offset` is an index into the creator's personal raffle list
    /// (not the global stable-ID space).  `params.limit` is clamped by
    /// `effective_limit` (1–200, default 100).
    pub fn get_raffles_by_creator(
        env: Env,
        creator: Address,
        params: PaginationParams,
    ) -> PageResultRaffles {
        let creator_raffles: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::CreatorRaffles(creator))
            .unwrap_or_else(|| Vec::new(&env));

        let total = creator_raffles.len();
        let lim = effective_limit(params.limit);
        let offset = params.offset;

        if offset >= total {
            return PageResultRaffles {
                items: Vec::new(&env),
                total,
                has_more: false,
            };
        }

        let end = offset.saturating_add(lim).min(total);
        let mut items: Vec<Address> = Vec::new(&env);
        for i in offset..end {
            items.push_back(creator_raffles.get(i).unwrap());
        }

        let has_more = end < total;
        PageResultRaffles {
            items,
            total,
            has_more,
        }
    }

    pub fn pause_factory(env: Env) -> Result<(), ContractError> {
        let admin = require_admin(&env)?;
        env.storage().instance().set(&DataKey::Paused, &true);

        events::ContractPaused {
            paused_by: admin,
            timestamp: env.ledger().timestamp(),
        }
        .publish(&env);

        Ok(())
    }

    pub fn unpause_factory(env: Env) -> Result<(), ContractError> {
        let admin = require_admin(&env)?;
        env.storage().instance().set(&DataKey::Paused, &false);

        events::ContractUnpaused {
            unpaused_by: admin,
            timestamp: env.ledger().timestamp(),
        }
        .publish(&env);

        Ok(())
    }

    pub fn is_factory_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    pub fn transfer_factory_admin(env: Env, new_admin: Address) -> Result<(), ContractError> {
        let admin = require_admin(&env)?;

        if new_admin == admin {
            env.storage().persistent().remove(&DataKey::PendingAdmin);
            return Ok(());
        }

        require_valid_role_address(&env, &new_admin)?;

        if env.storage().persistent().has(&DataKey::PendingAdmin) {
            return Err(ContractError::AdminTransferPending);
        }

        env.storage()
            .persistent()
            .set(&DataKey::PendingAdmin, &new_admin);

        events::AdminTransferProposed {
            current_admin: admin,
            proposed_admin: new_admin,
            timestamp: env.ledger().timestamp(),
        }
        .publish(&env);

        Ok(())
    }

    pub fn accept_factory_admin(env: Env) -> Result<(), ContractError> {
        let pending: Address = env
            .storage()
            .persistent()
            .get(&DataKey::PendingAdmin)
            .ok_or(ContractError::NoPendingTransfer)?;
        pending.require_auth();

        let old_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .ok_or(ContractError::NotAuthorized)?;

        env.storage().persistent().set(&DataKey::Admin, &pending);
        env.storage().persistent().remove(&DataKey::PendingAdmin);

        events::AdminTransferAccepted {
            old_admin,
            new_admin: pending,
            timestamp: env.ledger().timestamp(),
        }
        .publish(&env);

        Ok(())
    }

    pub fn get_checkpoint(env: Env, index: u32) -> Option<StateCheckpoint> {
        env.storage().persistent().get(&DataKey::Checkpoint(index))
    }

    pub fn get_latest_checkpoint_index(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::LatestCheckpointIndex)
            .unwrap_or(0u32)
    }

    pub fn sync_admin(env: Env, instance_address: Address) -> Result<(), ContractError> {
        let admin = require_admin(&env)?;
        env.invoke_contract::<()>(
            &instance_address,
            &Symbol::new(&env, "set_admin"),
            (admin,).into_val(&env),
        );
        Ok(())
    }

    pub fn pause_instance(env: Env, instance_address: Address) -> Result<(), ContractError> {
        require_admin(&env)?;
        env.invoke_contract::<()>(
            &instance_address,
            &Symbol::new(&env, "pause"),
            ().into_val(&env),
        );
        Ok(())
    }

    pub fn unpause_instance(env: Env, instance_address: Address) -> Result<(), ContractError> {
        require_admin(&env)?;
        env.invoke_contract::<()>(
            &instance_address,
            &Symbol::new(&env, "unpause"),
            ().into_val(&env),
        );
        Ok(())
    }

    pub fn track_participant(env: Env, participant: Address) -> Result<(), ContractError> {
        participant.require_auth();

        let key = DataKey::UniqueParticipant(participant.clone());
        if !env.storage().persistent().has(&key) {
            env.storage().persistent().set(&key, &true);
            let mut count: u32 = env
                .storage()
                .persistent()
                .get(&DataKey::TotalUniqueParticipants)
                .unwrap_or(0);
            count += 1;
            env.storage()
                .persistent()
                .set(&DataKey::TotalUniqueParticipants, &count);
        }
        Ok(())
    }

    pub fn get_unique_participants(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::TotalUniqueParticipants)
            .unwrap_or(0)
    }

    pub fn get_raffle_fairness_data(
        env: Env,
        raffle_id: Address,
    ) -> Result<FairnessData, ContractError> {
        Ok(env.invoke_contract::<FairnessData>(
            &raffle_id,
            &Symbol::new(&env, "get_fairness_data"),
            ().into_val(&env),
        ))
    }

    pub fn set_creation_delay(env: Env, delay_seconds: u64) -> Result<(), ContractError> {
        require_admin(&env)?;
        env.storage()
            .persistent()
            .set(&DataKey::MinCreationDelay, &delay_seconds);
        Ok(())
    }

    pub fn set_whitelist_status(
        env: Env,
        partner: Address,
        status: bool,
    ) -> Result<(), ContractError> {
        require_admin(&env)?;
        env.storage()
            .persistent()
            .set(&DataKey::WhitelistedPartner(partner), &status);
        Ok(())
    }

    /// Standard Soroban upgrade entry point for the factory contract WASM.
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) -> Result<(), ContractError> {
        let admin = require_admin(&env)?;
        env.deployer()
            .update_current_contract_wasm(new_wasm_hash.clone());

        events::FactoryUpgraded {
            admin,
            new_wasm_hash,
            timestamp: env.ledger().timestamp(),
        }
        .publish(&env);

        Ok(())
    }

    /// Sweep tokens accidentally sent to the factory contract.
    pub fn rescue_tokens(
        env: Env,
        token: Address,
        recipient: Address,
        amount: i128,
    ) -> Result<(), ContractError> {
        let admin = require_admin(&env)?;

        if amount <= 0 {
            return Err(ContractError::InvalidParameters);
        }

        let token_client = token::Client::new(&env, &token);
        let _ = token_client
            .try_transfer(&env.current_contract_address(), &recipient, &amount)
            .map_err(|_| ContractError::InvalidParameters)?;

        events::FactoryTokensRescued {
            rescued_by: admin,
            token,
            recipient,
            amount,
            timestamp: env.ledger().timestamp(),
        }
        .publish(&env);

        Ok(())
    }

    pub fn clean_old_raffle(env: Env, raffle_id: u32) -> Result<(), ContractError> {
        let admin = require_admin(&env)?;

        // Look up the raffle by its stable ID.  A missing entry means the ID
        // was never assigned or has already been cleaned up.
        let raffle_address: Address = env
            .storage()
            .persistent()
            .get(&DataKey::RaffleInstances)
            .unwrap_or_else(|| Vec::new(&env));

        if raffle_id >= instances.len() {
            return Err(ContractError::InvalidRaffleId);
        }

        let raffle_address = instances.get(raffle_id).unwrap();

            .get(&DataKey::RaffleById(raffle_id))
            .ok_or(ContractError::InvalidRaffleId)?;

        env.invoke_contract::<()>(
            &raffle_address,
            &Symbol::new(&env, "wipe_storage"),
            ().into_val(&env),
        );

        // Tombstone: remove the stable-map entry so the slot is freed and
        // `get_raffles_page` will skip it.  The stable_id is never reused so
        // other IDs are completely unaffected — no shifting, no reindexing.
        env.storage()
            .persistent()
            .remove(&DataKey::RaffleById(raffle_id));

        // Decrement the live count (floor at 0 for safety).
        let live_count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::RaffleCount)
            .unwrap_or(0u32);
        env.storage()
            .persistent()
            .set(&DataKey::RaffleCount, &live_count.saturating_sub(1));

        events::RaffleCleanedUp {
            raffle_address,
            cleaned_by: admin,
            finish_time: 0,
            cleaned_at: env.ledger().timestamp(),
        }
        .publish(&env);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use raffle_shared::{RandomnessSource, DEFAULT_PAGE_LIMIT, MAX_PAGE_LIMIT};
    use soroban_sdk::{String, Vec as SdkVec};

    fn setup_factory(env: &Env) -> (RaffleFactoryClient<'_>, Address, Address) {
        let admin = Address::generate(env);
        let treasury = Address::generate(env);
        let wasm_hash = BytesN::from_array(env, &[0u8; 32]);

        let contract_id = env.register(RaffleFactory, ());
        let client = RaffleFactoryClient::new(env, &contract_id);
        env.mock_all_auths();
        client.init_factory(&admin, &wasm_hash, &0u32, &treasury);
        client.set_creation_delay(&0u64);

        (client, admin, treasury)
    }

    fn test_raffle_config(env: &Env, payment_token: &Address) -> RaffleConfig {
        RaffleConfig {
            description: String::from_str(env, "Test Raffle"),
            end_time: 0,
            no_deadline: true,
            max_tickets: 10,
            max_tickets_per_tx: 10,
            min_tickets: 1,
            allow_multiple: true,
            ticket_price: 10_000,
            payment_token: payment_token.clone(),
            prize_amount: 10_000,
            prizes: SdkVec::from_array(env, [10_000u32]),
            randomness_source: RandomnessSource::Internal,
            oracle_address: None,
            protocol_fee_bp: 0,
            treasury_address: None,
            swap_router: None,
            tikka_token: None,
            metadata_hash: BytesN::from_array(env, &[1u8; 32]),
            claim_lockup_seconds: 0,
            swap_deadline_seconds: 0,
        }
    }

    fn create_raffles_via_factory(
        env: &Env,
        client: &RaffleFactoryClient<'_>,
        admin: &Address,
        treasury: &Address,
        creator: &Address,
        count: u32,
    ) -> SdkVec<Address> {
        use raffle_instance::ContractClient as RaffleInstanceClient;

        let factory_address = client.address.clone();
        let token_admin = Address::generate(env);
        let payment_token = env
            .register_stellar_asset_contract_v2(token_admin)
            .address();
        let protocol_fee_bp: u32 = env.as_contract(&factory_address, || {
            env.storage()
                .persistent()
                .get(&DataKey::ProtocolFeeBP)
                .unwrap_or(0)
        });

        let mut addrs = SdkVec::new(env);
        for _ in 0..count {
            let mut config = test_raffle_config(env, &payment_token);
            config.protocol_fee_bp = protocol_fee_bp;
            config.treasury_address = Some(treasury.clone());

            let raffle_address = env.register(raffle_instance::Contract, ());
            RaffleInstanceClient::new(env, &raffle_address).init(
                &factory_address,
                admin,
                creator,
                &config,
            );

            env.as_contract(&factory_address, || {
                let stable_id: u32 = env
                    .storage()
                    .persistent()
                    .get(&DataKey::NextRaffleId)
                    .unwrap_or(0u32);
                env.storage()
                    .persistent()
                    .set(&DataKey::RaffleById(stable_id), &raffle_address);
                env.storage()
                    .persistent()
                    .set(&DataKey::NextRaffleId, &(stable_id.saturating_add(1)));
                let live_count: u32 = env
                    .storage()
                    .persistent()
                    .get(&DataKey::RaffleCount)
                    .unwrap_or(0u32)
                    .saturating_add(1);
                env.storage()
                    .persistent()
                    .set(&DataKey::RaffleCount, &live_count);
            });

            addrs.push_back(raffle_address);
        }
        addrs
    }

    #[test]
    fn test_init_factory() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _treasury) = setup_factory(&env);
        assert_eq!(client.get_admin(), admin);
    }

    #[test]
    fn test_record_volume_overflow() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);
        let asset = Address::generate(&env);

        client.record_volume(&asset, &(i128::MAX - 1));
        assert_eq!(client.get_total_volume(&asset), i128::MAX - 1);
        assert!(client.try_record_volume(&asset, &2).is_err());
        assert_eq!(client.get_total_volume(&asset), i128::MAX - 1);
    }

    #[test]
    fn test_set_config_rejects_excessive_protocol_fee() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, treasury) = setup_factory(&env);
        let excessive_fee = MAX_PROTOCOL_FEE_BP + 1;

        assert_eq!(
            client.try_set_config(&excessive_fee, &treasury),
            Err(Ok(ContractError::InvalidParameters))
        );
    }

    #[test]
    fn test_init_factory_rejects_second_call() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        let wasm_hash = BytesN::from_array(&env, &[0u8; 32]);
        let contract_id = env.register(RaffleFactory, ());
        let client = RaffleFactoryClient::new(&env, &contract_id);

        client.init_factory(&admin, &wasm_hash, &0u32, &treasury);
        assert_eq!(
            client.try_init_factory(&admin, &wasm_hash, &0u32, &treasury),
            Err(Ok(ContractError::AlreadyInitialized))
        );
    }

    /// Strkey of the all-zero contract id (the "zero address").
    const ZERO_CONTRACT: &str = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4";

    fn zero_address(env: &Env) -> Address {
        Address::from_string(&String::from_str(env, ZERO_CONTRACT))
    }

    #[test]
    fn test_init_factory_rejects_zero_admin() {
        let env = Env::default();
        env.mock_all_auths();
        let treasury = Address::generate(&env);
        let wasm_hash = BytesN::from_array(&env, &[0u8; 32]);
        let contract_id = env.register(RaffleFactory, ());
        let client = RaffleFactoryClient::new(&env, &contract_id);

        assert_eq!(
            client.try_init_factory(&zero_address(&env), &wasm_hash, &0u32, &treasury),
            Err(Ok(ContractError::InvalidParameters))
        );
    }

    #[test]
    fn test_init_factory_rejects_zero_treasury() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let wasm_hash = BytesN::from_array(&env, &[0u8; 32]);
        let contract_id = env.register(RaffleFactory, ());
        let client = RaffleFactoryClient::new(&env, &contract_id);

        assert_eq!(
            client.try_init_factory(&admin, &wasm_hash, &0u32, &zero_address(&env)),
            Err(Ok(ContractError::InvalidParameters))
        );
    }

    #[test]
    fn test_init_factory_rejects_self_admin() {
        let env = Env::default();
        env.mock_all_auths();
        let treasury = Address::generate(&env);
        let wasm_hash = BytesN::from_array(&env, &[0u8; 32]);
        let contract_id = env.register(RaffleFactory, ());
        let client = RaffleFactoryClient::new(&env, &contract_id);

        assert_eq!(
            client.try_init_factory(&contract_id, &wasm_hash, &0u32, &treasury),
            Err(Ok(ContractError::InvalidParameters))
        );
    }

    #[test]
    fn test_init_factory_rejects_self_treasury() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let wasm_hash = BytesN::from_array(&env, &[0u8; 32]);
        let contract_id = env.register(RaffleFactory, ());
        let client = RaffleFactoryClient::new(&env, &contract_id);

        assert_eq!(
            client.try_init_factory(&admin, &wasm_hash, &0u32, &contract_id),
            Err(Ok(ContractError::InvalidParameters))
        );
    }

    #[test]
    fn test_transfer_factory_admin_rejects_zero_admin() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);

        assert_eq!(
            client.try_transfer_factory_admin(&zero_address(&env)),
            Err(Ok(ContractError::InvalidParameters))
        );
    }

    #[test]
    fn test_transfer_factory_admin_rejects_self() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);
        let self_address = client.address.clone();

        assert_eq!(
            client.try_transfer_factory_admin(&self_address),
            Err(Ok(ContractError::InvalidParameters))
        );
    }

    #[test]
    fn test_set_config_rejects_zero_treasury() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);

        assert_eq!(
            client.try_set_config(&0u32, &zero_address(&env)),
            Err(Ok(ContractError::InvalidParameters))
        );
    }

    #[test]
    fn test_set_config_rejects_self_treasury() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);
        let self_address = client.address.clone();

        assert_eq!(
            client.try_set_config(&0u32, &self_address),
            Err(Ok(ContractError::InvalidParameters))
        );
    }

    #[test]
    fn test_upgrade_requires_admin_authorization() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        let wasm_hash = BytesN::from_array(&env, &[0u8; 32]);
        let contract_id = env.register(RaffleFactory, ());
        let client = RaffleFactoryClient::new(&env, &contract_id);
        env.mock_all_auths();
        client.init_factory(&admin, &wasm_hash, &0u32, &treasury);

        let new_hash = BytesN::from_array(&env, &[9u8; 32]);
        // Without auth for the admin address, upgrade must not succeed.
        env.set_auths(&[]);
        assert!(client.try_upgrade(&new_hash).is_err());
    }

    // -----------------------------------------------------------------------
    // Stable-index storage tests (new with #426)
    //
    // These tests exercise the new storage layout directly via `env.as_contract`
    // to avoid the Soroban limitation that `env.register_at` cannot be called
    // from within an active contract invocation (which the test shim in
    // `create_raffle` does).  This approach tests the storage semantics cleanly.
    // -----------------------------------------------------------------------

    /// Seed the factory's stable-map storage with `n` synthetic raffle entries.
    fn seed_raffles(env: &Env, factory_id: &Address, n: u32) -> Vec<Address> {
        let mut addrs = Vec::new(env);
        env.as_contract(factory_id, || {
            for i in 0..n {
                let addr = Address::generate(env);
                env.storage()
                    .persistent()
                    .set(&DataKey::RaffleById(i), &addr);
                addrs.push_back(addr);
            }
            env.storage().persistent().set(&DataKey::NextRaffleId, &n);
            env.storage().persistent().set(&DataKey::RaffleCount, &n);
        });
        addrs
    }

    #[test]
    fn test_stable_ids_initial_state() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);

        // Before any raffle: NextRaffleId == 0, RaffleCount == 0.
        assert_eq!(client.get_next_raffle_id(), 0u32);
        assert_eq!(client.get_raffle_count(), 0u32);
        assert_eq!(client.get_raffle_by_id(&0u32), None);
    }

    #[test]
    fn test_stable_ids_seeded_lookup() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);
        let addrs = seed_raffles(&env, &client.address, 3);

        assert_eq!(client.get_next_raffle_id(), 3u32);
        assert_eq!(client.get_raffle_count(), 3u32);
        assert_eq!(client.get_raffle_by_id(&0u32), Some(addrs.get(0).unwrap()));
        assert_eq!(client.get_raffle_by_id(&1u32), Some(addrs.get(1).unwrap()));
        assert_eq!(client.get_raffle_by_id(&2u32), Some(addrs.get(2).unwrap()));
        // Non-existent ID returns None.
        assert_eq!(client.get_raffle_by_id(&99u32), None);
    }

    #[test]
    fn test_get_raffles_page_returns_correct_slice() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);
        let addrs = seed_raffles(&env, &client.address, 5);

        // Page 0: offset=0, limit=3 → IDs 0,1,2.
        let page = client.get_raffles_page(&raffle_shared::PaginationParams {
            limit: 3,
            offset: 0,
        });
        assert_eq!(page.items.len(), 3u32);
        assert_eq!(page.items.get(0).unwrap(), addrs.get(0).unwrap());
        assert_eq!(page.items.get(2).unwrap(), addrs.get(2).unwrap());
        assert!(page.has_more);

        // Page 1: offset=3, limit=3 → IDs 3,4 (only 2 remain).
        let page2 = client.get_raffles_page(&raffle_shared::PaginationParams {
            limit: 3,
            offset: 3,
        });
        assert_eq!(page2.items.len(), 2u32);
        assert_eq!(page2.items.get(0).unwrap(), addrs.get(3).unwrap());
        assert_eq!(page2.items.get(1).unwrap(), addrs.get(4).unwrap());
        assert!(!page2.has_more);

        // Out-of-range offset → empty.
        let page3 = client.get_raffles_page(&raffle_shared::PaginationParams {
            limit: 10,
            offset: 99,
        });
        assert_eq!(page3.items.len(), 0u32);
        assert!(!page3.has_more);
    }

    #[test]
    fn test_get_raffles_page_skips_tombstoned_slots() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);
        let addrs = seed_raffles(&env, &client.address, 3);

        // Tombstone slot 1 directly in storage.
        env.as_contract(&client.address, || {
            env.storage()
                .persistent()
                .remove(&DataKey::RaffleById(1u32));
            let count: u32 = env
                .storage()
                .persistent()
                .get(&DataKey::RaffleCount)
                .unwrap_or(0);
            env.storage()
                .persistent()
                .set(&DataKey::RaffleCount, &count.saturating_sub(1));
        });

        assert_eq!(client.get_raffle_count(), 2u32);
        assert_eq!(client.get_next_raffle_id(), 3u32); // monotonic, unchanged
        assert_eq!(client.get_raffle_by_id(&1u32), None);

        // Page over all IDs; tombstoned slot 1 is skipped.
        let page = client.get_raffles_page(&raffle_shared::PaginationParams {
            limit: 10,
            offset: 0,
        });
        assert_eq!(page.items.len(), 2u32);
        assert_eq!(page.items.get(0).unwrap(), addrs.get(0).unwrap());
        assert_eq!(page.items.get(1).unwrap(), addrs.get(2).unwrap());
    }

    #[test]
    fn get_raffles_page_empty_list() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);

        let page = client.get_raffles_page(&PaginationParams {
            limit: 10,
            offset: 0,
        });
        assert_eq!(page.items.len(), 0u32);
        assert_eq!(page.total, 0u32);
        assert!(!page.has_more);
    }

    #[test]
    fn get_raffles_page_first_page() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);
        let creator = Address::generate(&env);
        create_raffles_via_factory(&env, &client, &_admin, &_treasury, &creator, 15);

        let page = client.get_raffles_page(&PaginationParams {
            limit: 10,
            offset: 0,
        });
        assert_eq!(page.items.len(), 10u32);
        assert_eq!(page.total, 15u32);
        assert!(page.has_more);
    }

    #[test]
    fn get_raffles_page_last_page() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);
        let creator = Address::generate(&env);
        create_raffles_via_factory(&env, &client, &_admin, &_treasury, &creator, 15);

        let page = client.get_raffles_page(&PaginationParams {
            limit: 10,
            offset: 10,
        });
        assert_eq!(page.items.len(), 5u32);
        assert_eq!(page.total, 15u32);
        assert!(!page.has_more);
    }

    #[test]
    fn get_raffles_page_offset_beyond_total() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);
        let creator = Address::generate(&env);
        create_raffles_via_factory(&env, &client, &_admin, &_treasury, &creator, 5);

        let page = client.get_raffles_page(&PaginationParams {
            limit: 10,
            offset: 10,
        });
        assert_eq!(page.items.len(), 0u32);
        assert_eq!(page.total, 5u32);
        assert!(!page.has_more);
    }

    #[test]
    fn get_raffles_page_limit_zero_uses_default() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);
        let creator = Address::generate(&env);
        create_raffles_via_factory(&env, &client, &_admin, &_treasury, &creator, 150);

        let page = client.get_raffles_page(&PaginationParams {
            limit: 0,
            offset: 0,
        });
        assert_eq!(page.items.len(), DEFAULT_PAGE_LIMIT);
        assert_eq!(page.total, 150u32);
        assert!(page.has_more);
    }

    #[test]
    fn get_raffles_page_limit_above_max_is_clamped() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);
        let creator = Address::generate(&env);
        create_raffles_via_factory(&env, &client, &_admin, &_treasury, &creator, 250);

        let page = client.get_raffles_page(&PaginationParams {
            limit: 999,
            offset: 0,
        });
        assert_eq!(page.items.len(), MAX_PAGE_LIMIT);
        assert_eq!(page.total, 250u32);
        assert!(page.has_more);
    }

    #[test]
    fn test_clean_old_raffle_invalid_id_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);

        // No raffles → any ID is invalid.
        assert_eq!(
            client.try_clean_old_raffle(&0u32),
            Err(Ok(ContractError::InvalidRaffleId))
        );
    }

    #[test]
    fn test_clean_old_raffle_already_tombstoned_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);
        seed_raffles(&env, &client.address, 3);

        // Tombstone slot 1.
        env.as_contract(&client.address, || {
            env.storage()
                .persistent()
                .remove(&DataKey::RaffleById(1u32));
        });

        // Trying to clean it again must return InvalidRaffleId.
        assert_eq!(
            client.try_clean_old_raffle(&1u32),
            Err(Ok(ContractError::InvalidRaffleId))
        );
    }

    // -----------------------------------------------------------------------
    // Creator index tests
    // -----------------------------------------------------------------------

    /// Seed the per-creator index directly in storage with `addrs`.
    fn seed_creator_index(env: &Env, factory_id: &Address, creator: &Address, addrs: &[Address]) {
        env.as_contract(factory_id, || {
            let mut v: Vec<Address> = Vec::new(env);
            for a in addrs {
                v.push_back(a.clone());
            }
            env.storage()
                .persistent()
                .set(&DataKey::CreatorRaffles(creator.clone()), &v);
        });
    }

    #[test]
    fn test_get_raffles_by_creator_empty() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);
        let creator = Address::generate(&env);

        let page = client.get_raffles_by_creator(
            &creator,
            &raffle_shared::PaginationParams { limit: 10, offset: 0 },
        );
        assert_eq!(page.items.len(), 0u32);
        assert_eq!(page.total, 0u32);
        assert!(!page.has_more);
    }

    #[test]
    fn test_get_raffles_by_creator_basic() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);

        let creator_a = Address::generate(&env);
        let creator_b = Address::generate(&env);

        // 5 raffles for A, 3 for B.
        let mut a_addrs = [
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
        ];
        let b_addrs = [
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
        ];

        seed_creator_index(&env, &client.address, &creator_a, &a_addrs);
        seed_creator_index(&env, &client.address, &creator_b, &b_addrs);

        // Creator A: full page.
        let page_a = client.get_raffles_by_creator(
            &creator_a,
            &raffle_shared::PaginationParams { limit: 10, offset: 0 },
        );
        assert_eq!(page_a.total, 5u32);
        assert_eq!(page_a.items.len(), 5u32);
        assert!(!page_a.has_more);
        for (i, addr) in a_addrs.iter().enumerate() {
            assert_eq!(page_a.items.get(i as u32).unwrap(), addr.clone());
        }

        // Creator B: full page.
        let page_b = client.get_raffles_by_creator(
            &creator_b,
            &raffle_shared::PaginationParams { limit: 10, offset: 0 },
        );
        assert_eq!(page_b.total, 3u32);
        assert_eq!(page_b.items.len(), 3u32);
        assert!(!page_b.has_more);
        for (i, addr) in b_addrs.iter().enumerate() {
            assert_eq!(page_b.items.get(i as u32).unwrap(), addr.clone());
        }
    }

    #[test]
    fn test_get_raffles_by_creator_pagination() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);

        let creator = Address::generate(&env);
        let addrs = [
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
        ];
        seed_creator_index(&env, &client.address, &creator, &addrs);

        // Page 0: offset=0, limit=3 → items 0,1,2; has_more=true.
        let p0 = client.get_raffles_by_creator(
            &creator,
            &raffle_shared::PaginationParams { limit: 3, offset: 0 },
        );
        assert_eq!(p0.items.len(), 3u32);
        assert_eq!(p0.total, 5u32);
        assert!(p0.has_more);
        assert_eq!(p0.items.get(0).unwrap(), addrs[0].clone());
        assert_eq!(p0.items.get(2).unwrap(), addrs[2].clone());

        // Page 1: offset=3, limit=3 → items 3,4; has_more=false.
        let p1 = client.get_raffles_by_creator(
            &creator,
            &raffle_shared::PaginationParams { limit: 3, offset: 3 },
        );
        assert_eq!(p1.items.len(), 2u32);
        assert_eq!(p1.total, 5u32);
        assert!(!p1.has_more);
        assert_eq!(p1.items.get(0).unwrap(), addrs[3].clone());
        assert_eq!(p1.items.get(1).unwrap(), addrs[4].clone());

        // Out-of-range offset → empty, has_more=false.
        let p_oor = client.get_raffles_by_creator(
            &creator,
            &raffle_shared::PaginationParams { limit: 10, offset: 99 },
        );
        assert_eq!(p_oor.items.len(), 0u32);
        assert!(!p_oor.has_more);

        // Exact boundary: offset=5 (== total) → empty.
        let p_exact = client.get_raffles_by_creator(
            &creator,
            &raffle_shared::PaginationParams { limit: 10, offset: 5 },
        );
        assert_eq!(p_exact.items.len(), 0u32);
        assert!(!p_exact.has_more);
    }

    #[test]
    fn test_creator_index_isolates_separate_creators() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _treasury) = setup_factory(&env);

        let creator_a = Address::generate(&env);
        let creator_b = Address::generate(&env);

        let a_addrs = [Address::generate(&env), Address::generate(&env)];
        let b_addrs = [Address::generate(&env)];

        seed_creator_index(&env, &client.address, &creator_a, &a_addrs);
        seed_creator_index(&env, &client.address, &creator_b, &b_addrs);

        // A sees only its own raffles.
        let pa = client.get_raffles_by_creator(
            &creator_a,
            &raffle_shared::PaginationParams { limit: 10, offset: 0 },
        );
        assert_eq!(pa.total, 2u32);

        // B sees only its own raffle.
        let pb = client.get_raffles_by_creator(
            &creator_b,
            &raffle_shared::PaginationParams { limit: 10, offset: 0 },
        );
        assert_eq!(pb.total, 1u32);
        assert_eq!(pb.items.get(0).unwrap(), b_addrs[0].clone());
    }
}
