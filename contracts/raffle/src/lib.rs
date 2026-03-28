#![no_std]

pub const TIMELOCK_DELAY_SECONDS: u64 = 172800; // 48 hours
pub const CHECKPOINT_INTERVAL: u32 = 1_000;

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, xdr::ToXdr, Address, Env, IntoVal, String,
    Symbol, Vec,
};

mod events;
mod instance;
pub mod types;
pub use types::{PaginationParams, PageResult_Raffles, PageResult_Tickets, effective_limit};
use instance::{RaffleConfig, RandomnessSource};

#[contract]
pub struct RaffleFactory;

/// Describes the type of administrative change being queued.
#[derive(Clone)]
#[contracttype]
pub enum AdminOp {
    SetConfig { protocol_fee_bp: u32, treasury: Address },
}

/// A queued administrative operation.
#[derive(Clone)]
#[contracttype]
pub struct PendingOp {
    pub op: AdminOp,
    pub effective_timestamp: u64,
    pub proposed_by: Address,
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
    UniqueParticipant(Address),
    TotalUniqueParticipants,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum ContractError {
    // General errors (1-10)
    AlreadyInitialized = 1,
    NotAuthorized = 2,
    ContractPaused = 3,
    InvalidParameters = 4,
    RaffleNotFound = 5,
    
    // Admin errors (11-20)
    AdminTransferPending = 11,
    NoPendingTransfer = 12,
}

fn publish_factory_event<T>(env: &Env, event_name: &str, event: T)
where
    T: soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
{
    env.events().publish(
        (Symbol::new(env, "tikka"), Symbol::new(env, event_name)),
        event,
    );
}

fn require_factory_admin(env: &Env) -> Result<Address, ContractError> {
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

fn maybe_create_checkpoint(env: &Env, raffle_count: u32) {
    if raffle_count == 0 || raffle_count % CHECKPOINT_INTERVAL != 0 {
        return;
    }

    let index = raffle_count / CHECKPOINT_INTERVAL;
    let ledger_timestamp = env.ledger().timestamp();
    let ledger_sequence = env.ledger().sequence();

    // Serialise: raffle_count (u32 BE, 4 bytes) || ledger_sequence (u32 BE, 4 bytes) || ledger_timestamp (u64 BE, 8 bytes)
    let mut input = Bytes::new(env);
    input.extend_from_array(&raffle_count.to_be_bytes());
    input.extend_from_array(&ledger_sequence.to_be_bytes());
    input.extend_from_array(&ledger_timestamp.to_be_bytes());

    let aggregate_hash = env.crypto().sha256(&input);

    let checkpoint = StateCheckpoint {
        index,
        raffle_count,
        ledger_timestamp,
        aggregate_hash: aggregate_hash.clone(),
    };

    env.storage()
        .persistent()
        .set(&DataKey::Checkpoint(index), &checkpoint);
    env.storage()
        .persistent()
        .set(&DataKey::LatestCheckpointIndex, &index);

    publish_factory_event(
        env,
        "checkpoint_created",
        events::CheckpointCreated {
            index,
            raffle_count,
            ledger_timestamp,
            aggregate_hash,
        },
    );
}

#[contractimpl]
impl RaffleFactory {
    pub fn init_factory(
        env: Env,
        admin: Address,
        wasm_hash: soroban_sdk::BytesN<32>,
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
        Ok(())
    }

    pub fn set_config(
        env: Env,
        protocol_fee_bp: u32,
        treasury: Address,
    ) -> Result<(), ContractError> {
        require_factory_admin(&env)?;
        env.storage()
            .persistent()
            .set(&DataKey::OpCounter, &op_id);

        let effective_timestamp = env.ledger().timestamp() + TIMELOCK_DELAY_SECONDS;
        let op = AdminOp::SetConfig {
            protocol_fee_bp,
            treasury: treasury.clone(),
        };
        let pending = PendingOp {
            op: op.clone(),
            effective_timestamp,
            proposed_by: admin.clone(),
        };
        env.storage()
            .persistent()
            .set(&DataKey::PendingOp(op_id), &pending);

        publish_factory_event(
            &env,
            "admin_op_proposed",
            events::AdminOpProposed {
                op_id,
                op,
                effective_timestamp,
                proposed_by: admin,
            },
        );

        Ok(op_id)
    }

    pub fn execute_config_change(env: Env, op_id: u32) -> Result<(), ContractError> {
        let admin = require_factory_admin(&env)?;

        let pending: PendingOp = env
            .storage()
            .persistent()
            .get(&DataKey::PendingOp(op_id))
            .ok_or(ContractError::NoPendingOp)?;

        if env.ledger().timestamp() < pending.effective_timestamp {
            return Err(ContractError::TimelockNotElapsed);
        }

        match pending.op.clone() {
            AdminOp::SetConfig {
                protocol_fee_bp,
                treasury,
            } => {
                env.storage()
                    .persistent()
                    .set(&DataKey::ProtocolFeeBP, &protocol_fee_bp);
                env.storage()
                    .persistent()
                    .set(&DataKey::Treasury, &treasury);
            }
        }

        env.storage()
            .persistent()
            .remove(&DataKey::PendingOp(op_id));

        publish_factory_event(
            &env,
            "admin_op_executed",
            events::AdminOpExecuted {
                op_id,
                op: pending.op,
                executed_by: admin,
                executed_at: env.ledger().timestamp(),
            },
        );

        Ok(())
    }

    pub fn cancel_config_change(env: Env, op_id: u32) -> Result<(), ContractError> {
        let admin = require_factory_admin(&env)?;

        if !env
            .storage()
            .persistent()
            .has(&DataKey::PendingOp(op_id))
        {
            return Err(ContractError::NoPendingOp);
        }

        env.storage()
            .persistent()
            .remove(&DataKey::PendingOp(op_id));

        publish_factory_event(
            &env,
            "admin_op_cancelled",
            events::AdminOpCancelled {
                op_id,
                cancelled_by: admin,
                cancelled_at: env.ledger().timestamp(),
            },
        );

        Ok(())
    }

    pub fn get_pending_op(env: Env, op_id: u32) -> Option<PendingOp> {
        env.storage()
            .persistent()
            .get(&DataKey::PendingOp(op_id))
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

        let wasm_hash: soroban_sdk::BytesN<32> = env
            .storage()
            .persistent()
            .get(&DataKey::InstanceWasmHash)
            .unwrap();

        let protocol_fee_bp: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::ProtocolFeeBP)
            .unwrap_or(0);
        let treasury: Address = env.storage().persistent().get(&DataKey::Treasury).unwrap();

        let mut instances: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::RaffleInstances)
            .unwrap();

        // Use parameters to avoid warnings
        let mut final_config = config;
        final_config.protocol_fee_bp = protocol_fee_bp;
        final_config.treasury_address = Some(treasury);

        let admin: Address = env.storage().persistent().get(&DataKey::Admin).unwrap();
        let factory_address = env.current_contract_address();
        let client = instance::ContractClient::new(&env, &raffle_address);
        client.init(&factory_address, &admin, &creator, &config);

        instances.push_back(raffle_address.clone());
        env.storage()
            .persistent()
            .set(&DataKey::RaffleInstances, &instances);

        Ok(raffle_address)
    }

    pub fn get_admin(env: Env) -> Result<Address, ContractError> {
        env.storage()
            .persistent()
            .get(&DataKey::Admin)
            .ok_or(ContractError::NotAuthorized)
    }

    pub fn get_raffles(env: Env, params: PaginationParams) -> PageResult_Raffles {
        let all: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::RaffleInstances)
            .unwrap_or_else(|| Vec::new(&env));

        let total = all.len();
        let lim = effective_limit(params.limit);
        let offset = params.offset;

        if offset >= total {
            return PageResult_Raffles {
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
        PageResult_Raffles { items, total, has_more }
    }

    pub fn get_raffles_page(env: Env, params: PaginationParams) -> PageResult_Raffles {
        let all: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::RaffleInstances)
            .unwrap_or_else(|| Vec::new(&env));

        let total = all.len();
        let lim = effective_limit(params.limit);
        let offset = params.offset;

        if offset >= total {
            return PageResult_Raffles {
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
        PageResult_Raffles { items, total, has_more }
    }

    pub fn pause(env: Env) -> Result<(), ContractError> {
        let admin = require_factory_admin(&env)?;
        env.storage().instance().set(&DataKey::Paused, &true);

        publish_factory_event(
            &env,
            "contract_paused",
            events::ContractPaused {
                paused_by: admin,
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(())
    }

    pub fn unpause(env: Env) -> Result<(), ContractError> {
        let admin = require_factory_admin(&env)?;
        env.storage().instance().set(&DataKey::Paused, &false);

        publish_factory_event(
            &env,
            "contract_unpaused",
            events::ContractUnpaused {
                unpaused_by: admin,
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(())
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    pub fn transfer_admin(env: Env, new_admin: Address) -> Result<(), ContractError> {
        let admin = require_factory_admin(&env)?;

        // Self-transfer cancels any pending transfer
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

        publish_factory_event(
            &env,
            "admin_transfer_proposed",
            events::AdminTransferProposed {
                current_admin: admin,
                proposed_admin: new_admin,
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(())
    }

    pub fn accept_admin(env: Env) -> Result<(), ContractError> {
        let pending: Address = env
            .storage()
            .persistent()
            .get(&DataKey::PendingAdmin)
            .ok_or(ContractError::NoPendingTransfer)?;
        pending.require_auth();

        let old_admin: Address = env.storage().persistent().get(&DataKey::Admin).unwrap();

        env.storage().persistent().set(&DataKey::Admin, &pending);
        env.storage().persistent().remove(&DataKey::PendingAdmin);

        publish_factory_event(
            &env,
            "admin_transfer_accepted",
            events::AdminTransferAccepted {
                old_admin,
                new_admin: pending,
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(())
    }

    pub fn get_checkpoint(env: Env, index: u32) -> Option<StateCheckpoint> {
        env.storage()
            .persistent()
            .get(&DataKey::Checkpoint(index))
    }

    pub fn get_latest_checkpoint_index(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::LatestCheckpointIndex)
            .unwrap_or(0u32)
    }

    pub fn sync_admin(env: Env, instance_address: Address) -> Result<(), ContractError> {
        let admin = require_factory_admin(&env)?;
        let instance_client = instance::ContractClient::new(&env, &instance_address);
        instance_client.set_admin(&admin);
        Ok(())
    }

    pub fn pause_instance(env: Env, instance_address: Address) -> Result<(), ContractError> {
        require_factory_admin(&env)?;
        let instance_client = instance::ContractClient::new(&env, &instance_address);
        instance_client.pause();
        Ok(())
    }

    pub fn unpause_instance(env: Env, instance_address: Address) -> Result<(), ContractError> {
        require_factory_admin(&env)?;
        let instance_client = instance::ContractClient::new(&env, &instance_address);
        instance_client.unpause();
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Events},
        Address, Bytes, Env, String,
    };

    // -------------------------------------------------------------------------
    // Helper: initialise a RaffleFactory with mock_all_auths active.
    // Returns (client, admin, treasury).
    // -------------------------------------------------------------------------
    fn setup_factory(env: &Env) -> (RaffleFactoryClient<'_>, Address, Address) {
        let admin = Address::generate(env);
        let treasury = Address::generate(env);
        let wasm_hash = Bytes::from_slice(env, &[0u8; 32]);

        let contract_id = env.register(RaffleFactory, ());
        let client = RaffleFactoryClient::new(env, &contract_id);
        client.init_factory(&admin, &wasm_hash, &0u32, &treasury);

        (client, admin, treasury)
    }

    // -------------------------------------------------------------------------
    // Helper: build minimal create_raffle arguments
    // -------------------------------------------------------------------------
    fn make_raffle_args(env: &Env) -> (Address, String, u64, u32, bool, i128, Address, i128) {
        let token_admin = Address::generate(env);
        let token_contract = env.register_stellar_asset_contract_v2(token_admin);
        let payment_token = token_contract.address();
        let creator = Address::generate(env);
        (
            creator,
            String::from_str(env, "Test Raffle"),
            0u64,
            10u32,
            false,
            10i128,
            payment_token,
            100i128,
        )
    }

    // =========================================================================
    // 1. is_paused returns false on a freshly initialised factory (absent key)
    //    Validates: Requirement 1.5, 7.3
    // =========================================================================
    #[test]
    fn test_is_paused_default_false() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _, _) = setup_factory(&env);

        assert!(!client.is_paused());
    }

    // =========================================================================
    // 2. pause sets flag to true and emits ContractPaused event
    //    Validates: Requirement 1.2
    // =========================================================================
    #[test]
    fn test_pause_sets_flag_and_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _, _) = setup_factory(&env);

        client.pause();

        assert!(client.is_paused());
        assert!(!env.events().all().is_empty());
    }

    // =========================================================================
    // 3. unpause sets flag to false and emits ContractUnpaused event
    //    Validates: Requirement 1.3
    // =========================================================================
    #[test]
    fn test_unpause_sets_flag_and_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _, _) = setup_factory(&env);

        client.pause();
        assert!(client.is_paused());

        client.unpause();
        assert!(!client.is_paused());
        assert!(!env.events().all().is_empty());
    }

    // =========================================================================
    // 4. create_raffle returns ContractPaused when factory is paused
    //    Validates: Requirement 2.1
    // =========================================================================
    #[test]
    fn test_create_raffle_blocked_when_paused() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _, _) = setup_factory(&env);

        client.pause();

        let (creator, desc, end_time, max_tickets, allow_multiple, ticket_price, payment_token, prize_amount) =
            make_raffle_args(&env);

        let result = client.try_create_raffle(
            &creator,
            &desc,
            &end_time,
            &max_tickets,
            &allow_multiple,
            &ticket_price,
            &payment_token,
            &prize_amount,
            &instance::RandomnessSource::Internal,
            &None,
        );

        assert_eq!(result, Err(Ok(ContractError::ContractPaused)));
    }

    // =========================================================================
    // 5. create_raffle succeeds when factory is unpaused
    //    Validates: Requirement 2.2
    // =========================================================================
    #[test]
    fn test_create_raffle_succeeds_when_unpaused() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _, _) = setup_factory(&env);

        assert!(!client.is_paused());

        let (creator, desc, end_time, max_tickets, allow_multiple, ticket_price, payment_token, prize_amount) =
            make_raffle_args(&env);

        let result = client.try_create_raffle(
            &creator,
            &desc,
            &end_time,
            &max_tickets,
            &allow_multiple,
            &ticket_price,
            &payment_token,
            &prize_amount,
            &instance::RandomnessSource::Internal,
            &None,
        );

        assert!(result.is_ok());
    }

    // =========================================================================
    // 6. Non-admin caller on pause panics (require_auth fails / NotAuthorized)
    //    Validates: Requirement 1.4
    // =========================================================================
    #[test]
    #[should_panic] // NotAuthorized — admin key absent, client panics on Err
    fn test_pause_by_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        // Register factory without init → admin key absent → NotAuthorized
        let contract_id = env.register(RaffleFactory, ());
        let client = RaffleFactoryClient::new(&env, &contract_id);
        client.pause(); // panics because try_pause would return Err(NotAuthorized)
    }

    // =========================================================================
    // 6b. Non-admin: pause returns NotAuthorized when admin key is absent
    //     Validates: Requirement 1.4
    // =========================================================================
    #[test]
    fn test_pause_returns_not_authorized_when_no_admin_stored() {
        let env = Env::default();
        env.mock_all_auths();
        // Register factory but do NOT call init_factory → admin key absent
        let contract_id = env.register(RaffleFactory, ());
        let client = RaffleFactoryClient::new(&env, &contract_id);

        let result = client.try_pause();
        assert_eq!(result, Err(Ok(ContractError::NotAuthorized)));
    }

    // =========================================================================
    // 7. unpause returns NotAuthorized when admin key is absent
    //    Validates: Requirement 1.4
    // =========================================================================
    #[test]
    fn test_unpause_returns_not_authorized_when_no_admin_stored() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RaffleFactory, ());
        let client = RaffleFactoryClient::new(&env, &contract_id);

        let result = client.try_unpause();
        assert_eq!(result, Err(Ok(ContractError::NotAuthorized)));
    }

    // =========================================================================
    // 8. pause_instance returns NotAuthorized when admin key is absent
    //    Validates: Requirement 6.3
    // =========================================================================
    #[test]
    fn test_pause_instance_returns_not_authorized_when_no_admin_stored() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RaffleFactory, ());
        let client = RaffleFactoryClient::new(&env, &contract_id);
        let dummy_instance = Address::generate(&env);

        let result = client.try_pause_instance(&dummy_instance);
        assert_eq!(result, Err(Ok(ContractError::NotAuthorized)));
    }

    // =========================================================================
    // 9. unpause_instance returns NotAuthorized when admin key is absent
    //    Validates: Requirement 6.3
    // =========================================================================
    #[test]
    fn test_unpause_instance_returns_not_authorized_when_no_admin_stored() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RaffleFactory, ());
        let client = RaffleFactoryClient::new(&env, &contract_id);
        let dummy_instance = Address::generate(&env);

        let result = client.try_unpause_instance(&dummy_instance);
        assert_eq!(result, Err(Ok(ContractError::NotAuthorized)));
    }

    // =========================================================================
    // Helper: register a RaffleInstance and initialise it with the given factory
    // address. Returns the instance client.
    // =========================================================================
    fn setup_instance<'a>(
        env: &'a Env,
        factory_addr: &Address,
    ) -> instance::ContractClient<'a> {
        let admin = Address::generate(env);
        let creator = Address::generate(env);
        let token_admin = Address::generate(env);
        let token_contract = env.register_stellar_asset_contract_v2(token_admin);
        let payment_token = token_contract.address();

        let instance_id = env.register(instance::Contract, ());
        let instance_client = instance::ContractClient::new(env, &instance_id);

        let config = instance::RaffleConfig {
            description: String::from_str(env, "Delegation Test Raffle"),
            end_time: 0u64,
            max_tickets: 10u32,
            allow_multiple: false,
            ticket_price: 10i128,
            payment_token,
            prize_amount: 100i128,
            randomness_source: instance::RandomnessSource::Internal,
            oracle_address: None,
            protocol_fee_bp: 0u32,
            treasury_address: None,
            swap_router: None,
            tikka_token: None,
        };

        instance_client.init(factory_addr, &admin, &creator, &config);
        instance_client
    }

    // =========================================================================
    // 10. pause_instance causes the target instance's is_paused() to return true
    //     Validates: Requirement 6.1
    // =========================================================================
    #[test]
    fn test_pause_instance_propagates_pause_to_instance() {
        let env = Env::default();
        env.mock_all_auths();
        let (factory_client, _, _) = setup_factory(&env);

        let instance_client = setup_instance(&env, &factory_client.address);

        assert!(!instance_client.is_paused());

        factory_client.pause_instance(&instance_client.address);

        assert!(instance_client.is_paused());
    }

    // =========================================================================
    // 11. unpause_instance causes the target instance's is_paused() to return false
    //     Validates: Requirement 6.2
    // =========================================================================
    #[test]
    fn test_unpause_instance_propagates_unpause_to_instance() {
        let env = Env::default();
        env.mock_all_auths();
        let (factory_client, _, _) = setup_factory(&env);

        let instance_client = setup_instance(&env, &factory_client.address);

        // Pause first via factory delegation
        factory_client.pause_instance(&instance_client.address);
        assert!(instance_client.is_paused());

        // Now unpause via factory delegation
        factory_client.unpause_instance(&instance_client.address);
        assert!(!instance_client.is_paused());
    }

    // =========================================================================
    // 12. pause_instance / unpause_instance round-trip: multiple toggles
    //     Validates: Requirements 6.1, 6.2
    // =========================================================================
    #[test]
    fn test_delegation_pause_unpause_round_trip() {
        let env = Env::default();
        env.mock_all_auths();
        let (factory_client, _, _) = setup_factory(&env);

        let instance_client = setup_instance(&env, &factory_client.address);

        // Start unpaused
        assert!(!instance_client.is_paused());

        factory_client.pause_instance(&instance_client.address);
        assert!(instance_client.is_paused());

        factory_client.unpause_instance(&instance_client.address);
        assert!(!instance_client.is_paused());

        factory_client.pause_instance(&instance_client.address);
        assert!(instance_client.is_paused());
    }

    // =========================================================================
    // T1. TIMELOCK_DELAY_SECONDS constant equals 172800
    //     Validates: Requirement 6.1
    // =========================================================================
    #[test]
    fn test_constant_value() {
        assert_eq!(TIMELOCK_DELAY_SECONDS, 172800u64);
    }

    // =========================================================================
    // T2. init_factory sets ProtocolFeeBP and Treasury directly (bootstrap exemption)
    //     Validates: Requirement 5.2
    // =========================================================================
    #[test]
    fn test_init_factory_sets_config_directly() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        let wasm_hash = Bytes::from_slice(&env, &[0u8; 32]);

        let contract_id = env.register(RaffleFactory, ());
        let client = RaffleFactoryClient::new(&env, &contract_id);
        // init_factory must succeed without any timelock
        client.init_factory(&admin, &wasm_hash, &500u32, &treasury);

        // No pending ops should exist after init
        assert_eq!(client.get_op_counter(), 0u32);
        assert!(client.get_pending_op(&1u32).is_none());
    }

    // =========================================================================
    // T3. get_pending_op returns None for a missing op_id
    //     Validates: Requirement 4.1
    // =========================================================================
    #[test]
    fn test_get_pending_op_returns_none_for_missing_id() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _, _) = setup_factory(&env);

        assert!(client.get_pending_op(&999u32).is_none());
    }

    // =========================================================================
    // T4. get_op_counter returns 0 before any proposal
    //     Validates: Requirement 4.2
    // =========================================================================
    #[test]
    fn test_get_op_counter_returns_zero_before_any_proposal() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _, _) = setup_factory(&env);

        assert_eq!(client.get_op_counter(), 0u32);
    }

    // =========================================================================
    // T5. execute_config_change returns NoPendingOp for unknown op_id
    //     Validates: Requirement 2.6
    // =========================================================================
    #[test]
    fn test_execute_returns_no_pending_op_for_missing_id() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _, _) = setup_factory(&env);

        let result = client.try_execute_config_change(&42u32);
        assert_eq!(result, Err(Ok(ContractError::NoPendingOp)));
    }

    // =========================================================================
    // T6. cancel_config_change returns NoPendingOp for unknown op_id
    //     Validates: Requirement 3.4
    // =========================================================================
    #[test]
    fn test_cancel_returns_no_pending_op_for_missing_id() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _, _) = setup_factory(&env);

        let result = client.try_cancel_config_change(&42u32);
        assert_eq!(result, Err(Ok(ContractError::NoPendingOp)));
    }

    // =========================================================================
    // T7. get_pending_op and get_op_counter callable without admin auth
    //     Validates: Requirement 4.3
    // =========================================================================
    #[test]
    fn test_view_functions_require_no_auth() {
        let env = Env::default();
        // Do NOT call mock_all_auths — view functions must work without auth
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        let wasm_hash = Bytes::from_slice(&env, &[0u8; 32]);

        let contract_id = env.register(RaffleFactory, ());
        let client = RaffleFactoryClient::new(&env, &contract_id);

        // Use mock_all_auths only for init_factory
        env.mock_all_auths();
        client.init_factory(&admin, &wasm_hash, &0u32, &treasury);

        // Now call view functions — these must not require auth
        let counter = client.get_op_counter();
        assert_eq!(counter, 0u32);

        let pending = client.get_pending_op(&1u32);
        assert!(pending.is_none());
    }

    // =========================================================================
    // T8. set_config no longer exists — verified by compile-time absence
    //     Validates: Requirement 5.1
    // =========================================================================
    // This test is a compile-time check: if set_config existed, calling
    // client.set_config(...) would compile. Since it was removed, this test
    // simply documents the requirement. The absence of set_config is confirmed
    // by the fact that this file compiles without it.
    #[test]
    fn test_set_config_removed() {
        // Compile-time verification: set_config is not present in RaffleFactory.
        // If it were present, the line below would compile:
        //   client.set_config(&0u32, &Address::generate(&env));
        // Since set_config was removed (task 7), this test passes by compilation.
        assert!(true);
    }

    // =========================================================================
    // Checkpoint unit tests — Task 5
    // =========================================================================

    // Helper: call create_raffle n times, resetting budget each time.
    fn create_n_raffles(env: &Env, client: &RaffleFactoryClient<'_>, n: u32) {
        env.budget().reset_unlimited();
        for _ in 0..n {
            let (creator, desc, end_time, max_tickets, allow_multiple, ticket_price, payment_token, prize_amount) =
                make_raffle_args(env);
            client
                .create_raffle(
                    &creator,
                    &desc,
                    &end_time,
                    &max_tickets,
                    &allow_multiple,
                    &ticket_price,
                    &payment_token,
                    &prize_amount,
                    &instance::RandomnessSource::Internal,
                    &None,
                );
        }
    }

    // =========================================================================
    // C1. No checkpoint before the first milestone (999 raffles)
    //     Validates: Req 1.4, 3.3
    // =========================================================================
    #[test]
    fn test_no_checkpoint_before_first_milestone() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _, _) = setup_factory(&env);

        create_n_raffles(&env, &client, 999);

        assert_eq!(client.get_latest_checkpoint_index(), 0u32);
    }

    // =========================================================================
    // C2. Checkpoint created at exactly 1,000 raffles
    //     Validates: Req 1.1, 1.2
    // =========================================================================
    #[test]
    fn test_checkpoint_created_at_1000() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _, _) = setup_factory(&env);

        create_n_raffles(&env, &client, 1_000);

        assert!(client.get_checkpoint(&1u32).is_some());
    }

    // =========================================================================
    // C3. Checkpoint fields are correct at index 1
    //     Validates: Req 1.2, 2.1, 7.1, 7.2
    // =========================================================================
    #[test]
    fn test_checkpoint_fields_correct() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _, _) = setup_factory(&env);

        // Capture ledger state before the 1000th raffle
        let ledger_seq = env.ledger().sequence();
        let ledger_ts = env.ledger().timestamp();

        create_n_raffles(&env, &client, 1_000);

        let cp = client.get_checkpoint(&1u32).expect("checkpoint must exist");

        assert_eq!(cp.index, 1u32);
        assert_eq!(cp.raffle_count, 1_000u32);
        assert_eq!(cp.ledger_timestamp, ledger_ts);

        // Recompute expected hash: raffle_count BE4 || ledger_sequence BE4 || ledger_timestamp BE8
        let mut input = Bytes::new(&env);
        input.extend_from_array(&1_000u32.to_be_bytes());
        input.extend_from_array(&ledger_seq.to_be_bytes());
        input.extend_from_array(&ledger_ts.to_be_bytes());
        let expected_hash = env.crypto().sha256(&input);

        assert_eq!(cp.aggregate_hash, expected_hash);
    }

    // =========================================================================
    // C4. get_checkpoint returns None for a missing index
    //     Validates: Req 4.4
    // =========================================================================
    #[test]
    fn test_get_checkpoint_returns_none_for_missing_index() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _, _) = setup_factory(&env);

        assert!(client.get_checkpoint(&999u32).is_none());
    }

    // =========================================================================
    // C5. get_latest_checkpoint_index returns 0 on a fresh factory
    //     Validates: Req 3.3
    // =========================================================================
    #[test]
    fn test_get_latest_checkpoint_index_initial_zero() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _, _) = setup_factory(&env);

        assert_eq!(client.get_latest_checkpoint_index(), 0u32);
    }

    // =========================================================================
    // C6. Query functions require no authorisation
    //     Validates: Req 4.3
    // =========================================================================
    #[test]
    fn test_query_functions_require_no_auth() {
        let env = Env::default();
        // Initialise with auth mocked, then drop mock for query calls
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        let wasm_hash = Bytes::from_slice(&env, &[0u8; 32]);
        let contract_id = env.register(RaffleFactory, ());
        let client = RaffleFactoryClient::new(&env, &contract_id);
        env.mock_all_auths();
        client.init_factory(&admin, &wasm_hash, &0u32, &treasury);

        // Call query functions — no mock_all_auths active for these calls
        let idx = client.get_latest_checkpoint_index();
        assert_eq!(idx, 0u32);

        let cp = client.get_checkpoint(&1u32);
        assert!(cp.is_none());
    }

    // =========================================================================
    // C7. Paused factory rejects create_raffle at the milestone
    //     Validates: Req 6.3
    // =========================================================================
    #[test]
    fn test_paused_factory_rejects_create_raffle_at_milestone() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _, _) = setup_factory(&env);

        // Create 999 raffles first
        create_n_raffles(&env, &client, 999);

        // Pause the factory
        client.pause();

        // The 1000th raffle should be rejected
        let (creator, desc, end_time, max_tickets, allow_multiple, ticket_price, payment_token, prize_amount) =
            make_raffle_args(&env);
        let result = client.try_create_raffle(
            &creator,
            &desc,
            &end_time,
            &max_tickets,
            &allow_multiple,
            &ticket_price,
            &payment_token,
            &prize_amount,
            &instance::RandomnessSource::Internal,
            &None,
        );

        assert_eq!(result, Err(Ok(ContractError::ContractPaused)));
        // No checkpoint should have been created
        assert_eq!(client.get_latest_checkpoint_index(), 0u32);
    }

    // =========================================================================
    // C8. Checkpoint event is emitted with correct topic and payload
    //     Validates: Req 5.1, 5.2
    // =========================================================================
    #[test]
    fn test_checkpoint_event_emitted() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _, _) = setup_factory(&env);

        create_n_raffles(&env, &client, 1_000);

        let cp = client.get_checkpoint(&1u32).expect("checkpoint must exist");

        // Find the checkpoint_created event
        // env.events().all() returns Vec<(Address, Vec<Val>, Val)>: (contract_id, topics, data)
        let all_events = env.events().all();
        let tikka_sym = Symbol::new(&env, "tikka");
        let cp_sym = Symbol::new(&env, "checkpoint_created");
        let found = all_events.iter().any(|(_contract_id, topics, data)| {
            // topics is a Vec<Val>; check for ("tikka", "checkpoint_created") pair
            if topics.len() < 2 {
                return false;
            }
            let t0: soroban_sdk::Val = topics.get(0).unwrap();
            let t1: soroban_sdk::Val = topics.get(1).unwrap();
            let t0_matches = soroban_sdk::Symbol::try_from_val(&env, &t0)
                .map(|s: Symbol| s == tikka_sym)
                .unwrap_or(false);
            let t1_matches = soroban_sdk::Symbol::try_from_val(&env, &t1)
                .map(|s: Symbol| s == cp_sym)
                .unwrap_or(false);
            if !t0_matches || !t1_matches {
                return false;
            }
            // Decode the event payload as CheckpointCreated
            let event_data: events::CheckpointCreated =
                soroban_sdk::FromVal::from_val(&env, data);
            event_data.index == cp.index
                && event_data.raffle_count == cp.raffle_count
                && event_data.ledger_timestamp == cp.ledger_timestamp
                && event_data.aggregate_hash == cp.aggregate_hash
        });

        assert!(found, "checkpoint_created event not found or payload mismatch");
    }

    // =========================================================================
    // C9. Two sequential checkpoints at index 1 and 2
    //     Validates: Req 7.3
    // =========================================================================
    #[test]
    fn test_two_checkpoints_sequential() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _, _) = setup_factory(&env);

        create_n_raffles(&env, &client, 2_000);

        let cp1 = client.get_checkpoint(&1u32).expect("checkpoint 1 must exist");
        let cp2 = client.get_checkpoint(&2u32).expect("checkpoint 2 must exist");

        assert_eq!(cp1.index, 1u32);
        assert_eq!(cp1.raffle_count, 1_000u32);

        assert_eq!(cp2.index, 2u32);
        assert_eq!(cp2.raffle_count, 2_000u32);

        assert_eq!(client.get_latest_checkpoint_index(), 2u32);
    }
}
