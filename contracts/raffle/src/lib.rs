#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, Bytes, BytesN, Env, IntoVal,
    Symbol, Vec,
};

#[cfg(not(test))]
use soroban_sdk::xdr::ToXdr;

#[cfg(test)]
use soroban_sdk::testutils::Address as _;

mod events;

use raffle_shared::{
    effective_limit, AdminOp, FairnessData, PageResultRaffles, PaginationParams, RaffleConfig,
};

pub const TIMELOCK_DELAY_SECONDS: u64 = 172800; // 48 hours
pub const CHECKPOINT_INTERVAL: u64 = 1_000;

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
    pub index: u64,
    pub raffle_count: u32,
    pub ledger_timestamp: u64,
    pub aggregate_hash: BytesN<32>,
}

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    RaffleInstances,
    InstanceWasmHash,
    ProtocolFeeBP,
    Treasury,
    Paused,
    PendingAdmin,
    PendingOp(u32),
    OpCounter,
    Checkpoint(u64),
    LatestCheckpointIndex,
    TotalRafflesCreated,
    UniqueParticipant(Address),
    TotalUniqueParticipants,
    MinCreationDelay,
    LastCreationTime(Address),
    WhitelistedPartner(Address),
    TotalVolumePerAsset(Address),
    RaffleInstancesCount,
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
    RaffleCountOverflow = 19,
}

#[contract]
pub struct RaffleFactory;

fn require_admin(env: &Env) -> Result<Address, ContractError> {
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKey::Admin)
        .ok_or(ContractError::NotAuthorized)?;
    admin.require_auth();
    Ok(admin)
}

fn require_factory_not_paused(env: &Env) -> Result<(), ContractError> {
    if env
        .storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false)
    {
        return Err(ContractError::ContractPaused);
    }
    Ok(())
}

fn require_registered_raffle(env: &Env, raffle_address: &Address) -> Result<(), ContractError> {
    raffle_address.require_auth();

    let instances: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::RaffleInstances)
        .unwrap_or_else(|| Vec::new(env));

    for instance in instances.iter() {
        if instance == *raffle_address {
            return Ok(());
        }
    }

    Err(ContractError::NotAuthorized)
}

fn maybe_create_checkpoint(env: &Env, raffle_count: u32) {
    if raffle_count == 0 || raffle_count % (CHECKPOINT_INTERVAL as u32) != 0 {
        return;
    }

    let index = u64::from(raffle_count) / CHECKPOINT_INTERVAL;
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
    .publish(env);
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
        if env.storage().persistent().has(&DataKey::Admin) {
            return Err(ContractError::AlreadyInitialized);
        }
        env.storage().persistent().set(&DataKey::Admin, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::InstanceWasmHash, &wasm_hash);
        env.storage()
            .persistent()
            .set(&DataKey::RaffleInstances, &Vec::<Address>::new(&env));
        env.storage()
            .persistent()
            .set(&DataKey::ProtocolFeeBP, &protocol_fee_bp);
        env.storage()
            .persistent()
            .set(&DataKey::Treasury, &treasury);

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
                let old_treasury: Address = env.storage().persistent().get(&DataKey::Treasury).unwrap();

                env.storage()
                    .persistent()
                    .set(&DataKey::ProtocolFeeBP, &protocol_fee_bp);
                env.storage()
                    .persistent()
                    .set(&DataKey::Treasury, &treasury);

                events::TreasuryChanged {
                    old_treasury,
                    new_treasury: treasury.clone(),
                    changed_by: admin.clone(),
                    timestamp: env.ledger().timestamp(),
                }.publish(&env);
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

        let mut instances: Vec<Address> = env
    .storage()
    .persistent()
    .get(&DataKey::RaffleInstances)
    .unwrap_or_else(|| Vec::new(&env));

        let mut final_config = config;
        final_config.protocol_fee_bp = protocol_fee_bp;
        final_config.treasury_address = Some(treasury);

        let admin: Address = env.storage().persistent().get(&DataKey::Admin).unwrap();
        let factory_address = env.current_contract_address();

        #[cfg(not(test))]
        let raffle_address = {
            let wasm_hash: BytesN<32> = env
                .storage()
                .persistent()
                .get(&DataKey::InstanceWasmHash)
                .unwrap();
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
            count = count.checked_add(1).expect("RaffleInstancesCount overflow");
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
            (factory_address, admin, creator, final_config).into_val(&env),
        );

        instances.push_back(raffle_address.clone());
        env.storage()
            .persistent()
            .set(&DataKey::RaffleInstances, &instances);

        let mut count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalRafflesCreated)
            .unwrap_or(0);
        count = count.checked_add(1).ok_or(ContractError::RaffleCountOverflow)?;
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
        let all: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::RaffleInstances)
            .unwrap_or_else(|| Vec::new(&env));

        let total = all.len();
        let lim = effective_limit(params.limit);
        let offset = params.offset;

        if offset >= total {
            return PageResultRaffles {
                items: Vec::new(&env),
                total,
                has_more: false,
            };
        }

        let end = (offset + lim).min(total);
        let mut items = Vec::new(&env);
        for i in offset..end {
            items.push_back(all.get(i).unwrap());
        }

        let has_more = (offset + items.len()) < total;
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

        let old_admin: Address = env.storage().persistent().get(&DataKey::Admin).unwrap();

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

    pub fn get_checkpoint(env: Env, index: u64) -> Option<StateCheckpoint> {
        env.storage().persistent().get(&DataKey::Checkpoint(index))
    }

    pub fn get_latest_checkpoint_index(env: Env) -> u64 {
        env.storage()
            .persistent()
            .get(&DataKey::LatestCheckpointIndex)
            .unwrap_or(0u64)
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
        token_client
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

        let mut instances: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::RaffleInstances)
            .unwrap_or_else(|| Vec::new(&env));

        if raffle_id >= instances.len() {
            return Err(ContractError::InvalidRaffleId);
        }

        let raffle_address = instances.get(raffle_id).unwrap();

        env.invoke_contract::<()>(
            &raffle_address,
            &Symbol::new(&env, "wipe_storage"),
            ().into_val(&env),
        );

        let last_index = instances.len().saturating_sub(1);
        if raffle_id != last_index {
            let last_item = instances.get(last_index).unwrap();
            instances.set(raffle_id, last_item);
        }
        instances.remove(last_index);
        env.storage()
            .persistent()
            .set(&DataKey::RaffleInstances, &instances);

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
    fn setup_factory(env: &Env) -> (RaffleFactoryClient<'_>, Address, Address) {
        let admin = Address::generate(env);
        let treasury = Address::generate(env);
        let wasm_hash = BytesN::from_array(env, &[0u8; 32]);

        let contract_id = env.register(RaffleFactory, ());
        let client = RaffleFactoryClient::new(env, &contract_id);
        client.init_factory(&admin, &wasm_hash, &0u32, &treasury);
        client.set_creation_delay(&0u64);

        (client, admin, treasury)
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
        let (client, _admin, _treasury) = setup_factory(&env);
        let asset = Address::generate(&env);

        assert_eq!(client.get_total_volume(&asset), 0);
        assert_eq!(client.record_volume(&asset, &i128::MAX), Ok(()));
        assert_eq!(client.get_total_volume(&asset), i128::MAX);
        assert_eq!(client.record_volume(&asset, &1), Err(ContractError::ArithmeticOverflow));
        assert_eq!(client.get_total_volume(&asset), i128::MAX);
    }
}
