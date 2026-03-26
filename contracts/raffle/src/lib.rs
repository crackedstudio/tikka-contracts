#![no_std]
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, xdr::ToXdr, Address, Bytes, Env, String,
    Symbol, Vec,
};

mod events;
mod instance;
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
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum ContractError {
    AlreadyInitialized = 1,
    NotAuthorized = 2,
    ContractPaused = 3,
    AdminTransferPending = 4,
    NoPendingTransfer = 5,
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
        .persistent()
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
        wasm_hash: Bytes,
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

    pub fn set_config(env: Env, protocol_fee_bp: u32, treasury: Address) -> Result<(), ContractError> {
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

        let _wasm_hash: Bytes = env
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

        let mut _salt_src = Vec::new(&env);
        _salt_src.push_back(creator.clone());
        let _salt = env.crypto().sha256(&creator.clone().to_xdr(&env));

        // Deployment logic placeholder

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
        };

        instances.push_back(creator.clone());
        env.storage()
            .persistent()
            .set(&DataKey::RaffleInstances, &instances);

        Ok(creator)
    }

    pub fn get_admin(env: Env) -> Result<Address, ContractError> {
        env.storage()
            .persistent()
            .get(&DataKey::Admin)
            .ok_or(ContractError::NotAuthorized)
    }

    pub fn get_raffles(env: Env) -> Vec<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::RaffleInstances)
            .unwrap_or_else(|| Vec::new(&env))
    }

    pub fn pause(env: Env) -> Result<(), ContractError> {
        let admin = require_factory_admin(&env)?;
        env.storage().persistent().set(&DataKey::Paused, &true);

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
        env.storage().persistent().set(&DataKey::Paused, &false);

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
            .persistent()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }
}
