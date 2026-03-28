#![no_std]
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

    TotalRafflesCreated,
    TotalVolumePerAsset(Address),
}

#[derive(Clone)]
#[contracttype]
pub struct ProtocolStats {
    pub total_raffles_created: u32,
    pub protocol_fee_bp: u32,
    pub paused: bool,

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
        let admin = require_factory_admin(&env)?;
        let old_fee_bp: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::ProtocolFeeBP)
            .unwrap_or(0);
        let old_treasury: Option<Address> = env.storage().persistent().get(&DataKey::Treasury);

        env.storage()
            .persistent()
            .set(&DataKey::ProtocolFeeBP, &protocol_fee_bp);
        env.storage()
            .persistent()
            .set(&DataKey::Treasury, &treasury);

        publish_factory_event(
            &env,
            "fee_updated",
            events::FeeUpdated {
                old_fee_bp,
                new_fee_bp: protocol_fee_bp,
                updated_by: admin.clone(),
                timestamp: env.ledger().timestamp(),
            },
        );

        publish_factory_event(
            &env,
            "treasury_updated",
            events::TreasuryUpdated {
                old_treasury,
                new_treasury: treasury,
                updated_by: admin,
                timestamp: env.ledger().timestamp(),
            },
        );

        Ok(())
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

        let _ = RaffleConfig {
            description,
            end_time,
            max_tickets,
            allow_multiple,
            ticket_price,
            payment_token,
            prize_amount,
            randomness_source,
            oracle_address,
            protocol_fee_bp,
            treasury_address: Some(treasury),
            swap_router: None,
            tikka_token: None,
        };

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


        // Update global stats
        let mut count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalRafflesCreated)
            .unwrap_or(0);
        count += 1;
        env.storage()
            .persistent()
            .set(&DataKey::TotalRafflesCreated, &count);

        Ok(creator)

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

        ProtocolStats {
            total_raffles_created,
            protocol_fee_bp,
            paused,
        }
    }

    pub fn get_total_volume(env: Env, asset: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::TotalVolumePerAsset(asset))
            .unwrap_or(0)
    }

    pub fn record_volume(env: Env, asset: Address, amount: i128) -> Result<(), ContractError> {
        // In a production environment, this should be restricted to authorized raffle instances
        // For now, we allow any caller to update the volume as requested by the task
        let mut total_volume: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalVolumePerAsset(asset.clone()))
            .unwrap_or(0);
        total_volume += amount;
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

    pub fn transfer_ownership(env: Env, new_owner: Address) -> Result<(), ContractError> {
        Self::transfer_admin(env, new_owner)
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

    pub fn accept_ownership(env: Env) -> Result<(), ContractError> {
        Self::accept_admin(env)
    }

    pub fn sync_admin(env: Env, instance_address: Address) -> Result<(), ContractError> {
        let admin = require_factory_admin(&env)?;
        env.invoke_contract::<()>(
            &instance_address,
            &Symbol::new(&env, "set_admin"),
            (admin,).into_val(&env),
        );
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
    // 13. set_config emits FeeUpdated and TreasuryUpdated events
    // =========================================================================
    #[test]
    fn test_set_config_emits_events() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _) = setup_factory(&env);

        let new_treasury = Address::generate(&env);
        let new_fee = 500u32;

        client.set_config(&new_fee, &new_treasury);

        let events = env.events().all();
        let mut found_fee = false;
        let mut found_treasury = false;

        for event in events.iter() {
            let topics = event.0;
            if topics.get(1).unwrap() == Symbol::new(&env, "fee_updated") {
                found_fee = true;
            }
            if topics.get(1).unwrap() == Symbol::new(&env, "treasury_updated") {
                found_treasury = true;
            }
        }

        assert!(found_fee, "FeeUpdated event not found");
        assert!(found_treasury, "TreasuryUpdated event not found");
    }

    // =========================================================================
    // 14. transfer_ownership and accept_ownership work (aliases)
    // =========================================================================
    #[test]
    fn test_ownership_transfer_aliases() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _) = setup_factory(&env);

        let new_owner = Address::generate(&env);

        // Propose
        client.transfer_ownership(&new_owner);

        // Accept
        env.as_contract(&new_owner, || {
            client.accept_ownership();
        });

        assert_eq!(client.get_admin(), new_owner);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn test_protocol_stats() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let treasury = Address::generate(&env);
        let wasm_hash = Bytes::from_array(&env, &[0u8; 32]);
        
        RaffleFactory::init_factory(env.clone(), admin.clone(), wasm_hash, 100, treasury.clone()).unwrap();
        
        let stats = RaffleFactory::get_protocol_stats(env.clone());
        assert_eq!(stats.total_raffles_created, 0);
        
        let creator = Address::generate(&env);
        env.mock_all_auths();
        
        RaffleFactory::create_raffle(
            env.clone(),
            creator.clone(),
            String::from_str(&env, "Test"),
            0,
            10,
            false,
            100,
            Address::generate(&env),
            1000,
            RandomnessSource::Internal,
            None
        ).unwrap();
        
        let stats = RaffleFactory::get_protocol_stats(env.clone());
        assert_eq!(stats.total_raffles_created, 1);
        
        let asset = Address::generate(&env);
        RaffleFactory::record_volume(env.clone(), asset.clone(), 500).unwrap();
        
        assert_eq!(RaffleFactory::get_total_volume(env.clone(), asset.clone()), 500);
        
        RaffleFactory::record_volume(env.clone(), asset.clone(), 300).unwrap();
        assert_eq!(RaffleFactory::get_total_volume(env.clone(), asset.clone()), 800);
    }
}
