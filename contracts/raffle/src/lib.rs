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
    UniqueParticipant(Address),
    TotalUniqueParticipants,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum ContractError {
    AlreadyInitialized = 1,
    NotAuthorized = 2,
    ContractPaused = 3,
    AdminTransferPending = 4,
    NoPendingTransfer = 5,
    InvalidRaffleId = 6,
    RaffleNotEligible = 7,
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
        require_factory_admin(&env)?;
        env.storage()
            .persistent()
            .set(&DataKey::ProtocolFeeBP, &protocol_fee_bp);
        env.storage()
            .persistent()
            .set(&DataKey::Treasury, &treasury);
        Ok(())
    }

    pub fn create_raffle(
        env: Env,
        creator: Address,
        description: String,
        end_time: u64,
        max_tickets: u32,
        allow_multiple: bool,
        ticket_price: i128,
        payment_token: Address,
        prize_amount: i128,
        randomness_source: RandomnessSource,
        oracle_address: Option<Address>,
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

        let mut salt_src = Vec::new(&env);
        salt_src.push_back(creator.to_val());
        salt_src.push_back(instances.len().into_val(&env));
        let salt = env.crypto().sha256(&salt_src.to_xdr(&env));

        let raffle_address = env
            .deployer()
            .with_current_contract(salt)
            .deploy_v2(wasm_hash, ());

        let config = RaffleConfig {
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
        };

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

    /// Deletes all on-chain data for a raffle that has been in a terminal state
    /// for more than 90 days (7,776,000 seconds), reclaiming storage rent.
    pub fn clean_old_raffle(env: Env, raffle_id: u32) -> Result<(), ContractError> {
        // 1. Auth — must be first
        let admin = require_factory_admin(&env)?;

        // 2. Bounds check
        let mut instances: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::RaffleInstances)
            .unwrap_or_else(|| Vec::new(&env));
        if raffle_id >= instances.len() {
            return Err(ContractError::InvalidRaffleId);
        }

        // 3. Eligibility: cross-call get_finish_time and check 90-day window
        let raffle_address = instances.get(raffle_id).unwrap();
        let instance_client = instance::ContractClient::new(&env, &raffle_address);
        let finish_time = instance_client
            .get_finish_time()
            .ok_or(ContractError::RaffleNotEligible)?;
        let now = env.ledger().timestamp();
        if now - finish_time < 7_776_000u64 {
            return Err(ContractError::RaffleNotEligible);
        }

        // 4. Wipe instance storage
        instance_client.wipe_storage();

        // 5. Registry compaction (swap-remove)
        instances.swap_remove(raffle_id);
        env.storage()
            .persistent()
            .set(&DataKey::RaffleInstances, &instances);

        // 6. Audit event
        publish_factory_event(
            &env,
            "raffle_cleaned_up",
            events::RaffleCleanedUp {
                raffle_address,
                cleaned_by: admin,
                finish_time,
                cleaned_at: now,
            },
        );

        Ok(())
    }
}
