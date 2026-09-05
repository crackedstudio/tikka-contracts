#![cfg(test)]

#[path = "tests/admin.rs"]
mod admin;
#[path = "tests/invariants.rs"]
mod invariants;
#[path = "tests/tickets.rs"]
mod tickets;

use super::*;
use ed25519_dalek::{Signer, SigningKey};
use raffle_shared::{DEFAULT_CLAIM_LOCKUP_SECONDS, DEFAULT_SWAP_DEADLINE_SECONDS};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::StellarAssetClient,
    xdr::ToXdr,
    Address, Bytes, BytesN, Env, String, IntoVal, Val, Symbol,
};

pub fn assert_event<T: IntoVal<Env, Val>>(
    env: &Env,
    expected_contract: &Address,
    expected_topic: &str,
    expected_payload: T,
) {
    let events = env.events().all();
    let last = events.last().unwrap();
    assert_eq!(&last.0, expected_contract);
    assert_eq!(last.1.get(0).unwrap(), Symbol::new(env, "tikka").into_val(env));
    assert_eq!(last.1.get(1).unwrap(), Symbol::new(env, expected_topic).into_val(env));
    assert_eq!(last.2, expected_payload.into_val(env));
}

fn assert_drawing_lock_cleared(env: &Env, contract_id: &Address) {
    let is_set: bool = env.as_contract(contract_id, || {
        env.storage()
            .instance()
            .get(&crate::DataKey::DrawingLock)
            .unwrap_or(false)
    });
    assert!(!is_set, "DrawingLock must be cleared");
}

#[test]
fn ticket_number_matches_monotonic_id() {
    let env = Env::default();
    let owner = Address::generate(&env);

    let ticket = Ticket::new(7, owner, 123);

    assert_eq!(ticket.id, 7);
    assert_eq!(ticket.ticket_number, 7);
    assert_eq!(ticket.ticket_number, ticket.id);
}

#[test]
fn test_oracle_fallback_with_ledger_delays() {
    let env = Env::default();
    env.mock_all_auths();

    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let oracle = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = StellarAssetClient::new(&env, &payment_token);
    token_client.mint(&creator, &100_000_000);

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Test Raffle"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 10,
        max_tickets_per_tx: 10,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: 10_000,
        payment_token: payment_token.clone(),
        prize_amount: 10_000,
        prizes: soroban_sdk::vec![&env, 10000],
        randomness_source: RandomnessSource::External,
        oracle_address: Some(oracle.clone()),
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        unique_winners: false,
        metadata_hash: BytesN::from_array(&env, &[1; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);

    let raffle = client.get_raffle();
    assert_eq!(raffle.claim_lockup_seconds, DEFAULT_CLAIM_LOCKUP_SECONDS);
    assert_eq!(raffle.swap_deadline_seconds, DEFAULT_SWAP_DEADLINE_SECONDS);

    env.as_contract(&contract_id, || {
        env.storage().instance().remove(&DataKey::Factory);
    });

    client.deposit_prize();
    client.buy_tickets(&creator, &10);

    client.finalize_raffle();

    let raffle = client.get_raffle();
    assert_eq!(raffle.status, RaffleStatus::Drawing);

    let start_events = env.events().all().len();
    let result = client.try_trigger_randomness_fallback(&creator, &false);
    assert_eq!(env.events().all().len(), start_events);
    assert_eq!(result.err(), Some(Ok(Error::FallbackTooEarly)));

    env.ledger().with_mut(|l| {
        l.sequence_number += ORACLE_TIMEOUT_LEDGERS + 1;
        l.timestamp += 86400;
    });

    client.trigger_randomness_fallback(&creator, &false);

    let raffle_after = client.get_raffle();
    assert_eq!(raffle_after.status, RaffleStatus::Finalized);

    let fairness = client.get_fairness_data();
    assert_eq!(fairness.randomness_source, RandomnessSource::External);
}

fn create_token<'a>(env: &'a Env, admin: &Address) -> (Address, StellarAssetClient<'a>) {
    let payment_token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    (
        payment_token.clone(),
        StellarAssetClient::new(env, &payment_token),
    )
}

#[contractimpl]
impl MockFactory {
    pub fn record_volume(_env: Env, _token: Address, _amount: i128) {}
    pub fn track_participant(_env: Env, _participant: Address) {}
}

#[test]
fn test_admin_updates_oracle_address() {
    let env = Env::default();
    env.mock_all_auths();

    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let oracle = Address::generate(&env);
    let new_oracle = Address::generate(&env);

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Oracle migration"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 5,
        max_tickets_per_tx: 5,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: env
            .register_stellar_asset_contract_v2(Address::generate(&env))
            .address(),
        prize_amount: MIN_TICKET_PRICE * 5,
        prizes: soroban_sdk::vec![&env, 10000],
        randomness_source: RandomnessSource::External,
        oracle_address: Some(oracle.clone()),
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        unique_winners: false,
        metadata_hash: BytesN::from_array(&env, &[2; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);

    let raffle = client.get_raffle();
    assert_eq!(raffle.claim_lockup_seconds, DEFAULT_CLAIM_LOCKUP_SECONDS);
    assert_eq!(raffle.swap_deadline_seconds, DEFAULT_SWAP_DEADLINE_SECONDS);

    client.update_oracle_address(&new_oracle);

    let raffle = client.get_raffle();
    assert_eq!(raffle.oracle_address, Some(new_oracle));
}

#[test]
fn test_admin_sets_protocol_fee_before_sales() {
    let env = Env::default();
    env.mock_all_auths();

    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let treasury = Address::generate(&env);

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Fee update"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 5,
        max_tickets_per_tx: 5,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: env
            .register_stellar_asset_contract_v2(Address::generate(&env))
            .address(),
        prize_amount: MIN_TICKET_PRICE * 5,
        prizes: soroban_sdk::vec![&env, 10000],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 100,
        treasury_address: Some(treasury),
        swap_router: None,
        tikka_token: None,
        unique_winners: false,
        metadata_hash: BytesN::from_array(&env, &[3; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);

    let raffle = client.get_raffle();
    assert_eq!(raffle.claim_lockup_seconds, DEFAULT_CLAIM_LOCKUP_SECONDS);
    assert_eq!(raffle.swap_deadline_seconds, DEFAULT_SWAP_DEADLINE_SECONDS);

    client.set_protocol_fee_bp(&500);

    let raffle = client.get_raffle();
    assert_eq!(raffle.protocol_fee_bp, 500);
}

#[test]
fn non_winner_cannot_claim() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let buyer = Address::generate(&env);
    let attacker = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let (token_addr, token_mint) = create_token(&env, &token_admin);
    token_mint.mint(&creator, &1_000_000);
    token_mint.mint(&buyer, &1_000_000);

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "test raffle"),
        end_time: 2_000,
        no_deadline: false,
        max_tickets: 2,
        max_tickets_per_tx: 2,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: token_addr.clone(),
        prize_amount: MIN_TICKET_PRICE * 10,
        prizes: soroban_sdk::vec![&env, 10000u32],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        unique_winners: false,
        metadata_hash: BytesN::from_array(&env, &[1u8; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();
    client.buy_tickets(&buyer, &1);
    env.ledger().set_timestamp(2_000);
    client.finalize_raffle();

    let raffle = client.get_raffle();
    assert_eq!(raffle.winners.len(), 1);
    assert!(raffle.winners.get(0).unwrap() != attacker);

    env.ledger()
        .set_timestamp(2_000 + DEFAULT_CLAIM_LOCKUP_SECONDS + 1);

    let start_events = env.events().all().len();
    let result = client.try_claim_prize(&attacker, &0u32);
    assert_eq!(env.events().all().len(), start_events);
    assert_eq!(result, Err(Ok(Error::NotWinner)));
}

#[test]
fn buy_tickets_rejects_quantity_above_per_tx_cap() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(&env, &contract_id);

    let factory = env.register(MockFactory, ());
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let buyer = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let (token_addr, token_mint) = create_token(&env, &token_admin);
    token_mint.mint(&creator, &1_000_000);
    token_mint.mint(&buyer, &1_000_000);

    let config = RaffleConfig {
        description: String::from_str(&env, "Per-tx cap"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 100,
        max_tickets_per_tx: 5,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: token_addr.clone(),
        prize_amount: MIN_TICKET_PRICE * 100,
        prizes: vec![&env, 10000u32],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        unique_winners: false,
        metadata_hash: BytesN::from_array(&env, &[5u8; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();

    let start_events = env.events().all().len();
    assert_eq!(
        client.try_buy_tickets(&buyer, &6),
        Err(Ok(Error::ExceedsMaxTicketsPerTx))
    );
    assert_eq!(env.events().all().len(), start_events);
    assert_eq!(client.buy_tickets(&buyer, &5), 5);
}

#[test]
fn buy_tickets_rejects_overflowing_total_price_without_wrapping() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(&env, &contract_id);

    let factory = env.register(MockFactory, ());
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let buyer = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let (token_addr, token_mint) = create_token(&env, &token_admin);
    token_mint.mint(&creator, &1_000_000_000_000);
    token_mint.mint(&buyer, &1_000_000_000_000);

    let quantity = 100_000u32;
    let config = RaffleConfig {
        description: String::from_str(&env, "overflow guard"),
        end_time: 0,
        no_deadline: true,
        max_tickets: quantity,
        max_tickets_per_tx: quantity,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: token_addr.clone(),
        prize_amount: MIN_TICKET_PRICE * 10,
        prizes: vec![&env, 10000u32],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        unique_winners: false,
        metadata_hash: BytesN::from_array(&env, &[77u8; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();

    let overflow_price = (i128::MAX / quantity as i128) + 1;
    env.as_contract(&contract_id, || {
        let mut raffle = crate::read_raffle(&env).unwrap();
        raffle.ticket_price = overflow_price;
        raffle.prize_amount = overflow_price;
        crate::write_raffle(&env, &raffle);
    });

    assert_eq!(
        client.try_buy_tickets(&buyer, &quantity),
        Err(Ok(Error::ArithmeticOverflow))
    );

    let raffle = client.get_raffle();
    assert_eq!(raffle.tickets_sold, 0);
}

fn setup_scale_raffle(
    env: &Env,
    max_tickets: u32,
    max_tickets_per_tx: u32,
    prize_amount: i128,
) -> (
    RaffleInstanceClient<'_>,
    Address,
    Address,
    Address,
    Address,
    token::StellarAssetClient<'_>,
) {
    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(env, &contract_id);
    let factory = env.register(MockFactory, ());
    let admin = Address::generate(env);
    let creator = Address::generate(env);
    let buyer = Address::generate(env);

    let token_admin = Address::generate(env);
    let (payment_token, token_mint) = create_token(env, &token_admin);
    token_mint.mint(&creator, &prize_amount * 2);
    token_mint.mint(&buyer, &prize_amount * 2);

    let config = RaffleConfig {
        description: String::from_str(env, "scale benchmark"),
        end_time: 0,
        no_deadline: true,
        max_tickets,
        max_tickets_per_tx,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: payment_token.clone(),
        prize_amount,
        prizes: vec![env, 10000u32],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        unique_winners: false,
        metadata_hash: BytesN::from_array(env, &[88u8; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);
    env.as_contract(&contract_id, || {
        env.storage().instance().remove(&DataKey::Factory);
    });
    client.deposit_prize();

    (
        client,
        contract_id,
        creator,
        buyer,
        payment_token,
        token_mint,
    )
}

fn record_costs<F: FnOnce()>(env: &Env, f: F) -> (u64, u64) {
    env.cost_estimate().budget().reset_default();
    f();
    let budget = env.cost_estimate().budget();
    (budget.cpu_instruction_cost(), budget.memory_bytes_cost())
}

const BUY_TICKETS_1K_CPU_CEILING: u64 = 30_000_000;
const BUY_TICKETS_1K_MEM_CEILING: u64 = 10 * 1024 * 1024;
const FINALIZE_10K_CPU_CEILING: u64 = 80_000_000;
const FINALIZE_10K_MEM_CEILING: u64 = 24 * 1024 * 1024;
const GET_MY_TICKETS_10K_CPU_CEILING: u64 = 15_000_000;
const GET_MY_TICKETS_10K_MEM_CEILING: u64 = 8 * 1024 * 1024;

#[test]
fn buy_tickets_cost_stays_below_ceiling_for_1k_batch() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let (client, _contract_id, buyer, _, _, _) =
        setup_scale_raffle(&env, 1_000, 1_000, MIN_TICKET_PRICE * 10);

    let (cpu, mem) = record_costs(&env, || {
        client.buy_tickets(&buyer, &1_000);
    });

    assert!(
        cpu < BUY_TICKETS_1K_CPU_CEILING,
        "buy_tickets 1k CPU {cpu} exceeded ceiling {BUY_TICKETS_1K_CPU_CEILING}"
    );
    assert!(
        mem < BUY_TICKETS_1K_MEM_CEILING,
        "buy_tickets 1k memory {mem} exceeded ceiling {BUY_TICKETS_1K_MEM_CEILING}"
    );
}

#[test]
fn finalize_raffle_cost_stays_below_ceiling_for_10k_tickets() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let (client, contract_id, buyer, _, _, _) =
        setup_scale_raffle(&env, 10_000, 1_000, MIN_TICKET_PRICE * 20);
    for _ in 0..10 {
        client.buy_tickets(&buyer, &1_000);
    }

    let (cpu, mem) = record_costs(&env, || {
        client.finalize_raffle();
    });

    assert!(
        cpu < FINALIZE_10K_CPU_CEILING,
        "finalize_raffle 10k CPU {cpu} exceeded ceiling {FINALIZE_10K_CPU_CEILING}"
    );
    assert!(
        mem < FINALIZE_10K_MEM_CEILING,
        "finalize_raffle 10k memory {mem} exceeded ceiling {FINALIZE_10K_MEM_CEILING}"
    );

    env.as_contract(&contract_id, || {
        let raffle = crate::read_raffle(&env).unwrap();
        assert_eq!(raffle.status, RaffleStatus::Finalized);
    });
}

#[test]
fn get_my_tickets_cost_stays_below_ceiling_for_10k_owned_tickets() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let (client, _contract_id, buyer, _, _, _) =
        setup_scale_raffle(&env, 10_000, 1_000, MIN_TICKET_PRICE * 20);
    for _ in 0..10 {
        client.buy_tickets(&buyer, &1_000);
    }

    let (cpu, mem) = record_costs(&env, || {
        let tickets = client.get_my_tickets(&buyer);
        assert_eq!(tickets.len(), 10_000);
    });

    assert!(
        cpu < GET_MY_TICKETS_10K_CPU_CEILING,
        "get_my_tickets 10k CPU {cpu} exceeded ceiling {GET_MY_TICKETS_10K_CPU_CEILING}"
    );
    assert!(
        mem < GET_MY_TICKETS_10K_MEM_CEILING,
        "get_my_tickets 10k memory {mem} exceeded ceiling {GET_MY_TICKETS_10K_MEM_CEILING}"
    );
}

fn setup_active_raffle(
    env: &Env,
) -> (
    RaffleInstanceClient<'_>,
    Address,
    Address,
    Address,
    Address,
    token::StellarAssetClient<'_>,
) {
    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(env, &contract_id);

    let factory = env.register(MockFactory, ());
    let admin = Address::generate(env);
    let creator = Address::generate(env);
    let buyer = Address::generate(env);

    let token_admin = Address::generate(env);
    let (token_addr, token_mint) = create_token(env, &token_admin);
    token_mint.mint(&creator, &1_000_000);
    token_mint.mint(&buyer, &1_000_000);

    let config = RaffleConfig {
        description: String::from_str(env, "ticket sales pause"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 1,
        max_tickets_per_tx: 1,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: token_addr,
        prize_amount: MIN_TICKET_PRICE * 100,
        prizes: vec![env, 10000u32],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        unique_winners: false,
        metadata_hash: BytesN::from_array(env, &[7u8; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();

    (client, admin, creator, buyer, factory, token_mint)
}

#[test]
fn pause_resume_ticket_sales_controls_buy_tickets() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let (client, _admin, _creator, buyer, _factory, _token_mint) = setup_active_raffle(&env);

    assert_eq!(client.get_raffle().status, RaffleStatus::Active);
    assert!(!client.is_ticket_sales_paused());

    client.pause_ticket_sales(&_admin);
    assert!(client.is_ticket_sales_paused());

    assert_eq!(
        client.try_buy_tickets(&buyer, &1),
        Err(Ok(Error::ContractPaused))
    );

    client.resume_ticket_sales(&_admin);
    assert!(!client.is_ticket_sales_paused());
    assert_eq!(client.get_raffle().status, RaffleStatus::Active);
    assert_eq!(client.buy_tickets(&buyer, &1), 1);
}

#[test]
fn admin_can_pause_and_resume_ticket_sales() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let (client, admin, _creator, buyer, _factory, _token_mint) = setup_active_raffle(&env);

    client.pause_ticket_sales(&admin);
    assert!(client.is_ticket_sales_paused());
    let start_events = env.events().all().len();
    assert_eq!(
        client.try_buy_tickets(&buyer, &1),
        Err(Ok(Error::ContractPaused))
    );
    assert_eq!(env.events().all().len(), start_events);

    client.resume_ticket_sales(&admin);
    assert!(!client.is_ticket_sales_paused());
    assert_eq!(client.buy_tickets(&buyer, &1), 1);
}

#[test]
fn test_wipe_storage_removes_all_keys() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(&env, &contract_id);

    let factory = env.register(MockFactory, ());
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let buyer_a = Address::generate(&env);
    let buyer_b = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let (token_addr, token_mint) = create_token(&env, &token_admin);
    token_mint.mint(&creator, &1_000_000);
    token_mint.mint(&buyer_a, &1_000_000);
    token_mint.mint(&buyer_b, &1_000_000);

    let config = RaffleConfig {
        description: String::from_str(&env, "wipe test"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 10,
        max_tickets_per_tx: 10,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: token_addr,
        prize_amount: MIN_TICKET_PRICE * 10,
        prizes: vec![&env, 10000u32],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        unique_winners: false,
        metadata_hash: BytesN::from_array(&env, &[9u8; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };
    let expected_metadata_hash = config.metadata_hash.clone();

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();
    client.buy_tickets(&buyer_a, &3);
    client.buy_tickets(&buyer_b, &2);
    assert_metadata_hash(&client, &expected_metadata_hash);

    client.cancel_raffle(&CancelReason::AdminCancelled);

    assert_eq!(client.get_raffle().status, RaffleStatus::Cancelled);
    assert_metadata_hash(&client, &expected_metadata_hash);

    assert_eq!(
        client.try_wipe_storage(),
        Err(Ok(Error::InvalidStateTransition))
    );

    env.as_contract(&contract_id, || {
        assert!(env.storage().instance().has(&DataKey::Raffle));
        assert!(env.storage().instance().has(&DataKey::Factory));
        assert!(env.storage().instance().has(&DataKey::Admin));
        assert!(env.storage().persistent().has(&DataKey::Ticket(1)));
        assert!(env.storage().persistent().has(&DataKey::TicketCount(buyer_a)));
        assert!(env.storage().persistent().has(&DataKey::TicketCount(buyer_b)));
        assert!(env.storage().persistent().has(&DataKey::TicketBuyers));
    });
}

#[test]
fn wipe_storage_rejects_every_non_terminal_status() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let (client, contract_id, _, _, _, _) = setup_active_raffle(&env);
    for status in [
        RaffleStatus::PendingPrize,
        RaffleStatus::Active,
        RaffleStatus::Drawing,
        RaffleStatus::Finalized,
    ] {
        env.as_contract(&contract_id, || {
            let mut raffle = crate::read_raffle(&env).unwrap();
            raffle.status = status.clone();
            crate::write_raffle(&env, &raffle);
        });
        assert_eq!(client.try_wipe_storage(), Err(Ok(Error::InvalidStatus)));
    }
}

#[test]
fn emergency_withdraw_fails_before_delay_in_finalized_state() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = StellarAssetClient::new(&env, &payment_token);
    token_client.mint(&creator, &10_000_000);

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Test"),
        end_time: 2_000,
        no_deadline: false,
        max_tickets: 1,
        max_tickets_per_tx: 1,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: payment_token.clone(),
        prize_amount: MIN_TICKET_PRICE * 10,
        prizes: soroban_sdk::vec![&env, 10000],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        unique_winners: false,
        metadata_hash: BytesN::from_array(&env, &[9; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();
    client.buy_tickets(&creator, &1);
    client.finalize_raffle();

    let start_events = env.events().all().len();
    let result = client.try_emergency_withdraw(&creator);
    assert_eq!(env.events().all().len(), start_events);
    assert_eq!(result.err(), Some(Ok(Error::InvalidStatus)));
}

#[test]
fn emergency_withdraw_rejects_finalized_state_after_delay() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = StellarAssetClient::new(&env, &payment_token);
    token_client.mint(&creator, &1_000_000);

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Test"),
        end_time: 2_000,
        no_deadline: false,
        max_tickets: 1,
        max_tickets_per_tx: 1,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: payment_token.clone(),
        prize_amount: MIN_TICKET_PRICE * 10,
        prizes: soroban_sdk::vec![&env, 10000],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        unique_winners: false,
        metadata_hash: BytesN::from_array(&env, &[10; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();
    client.buy_tickets(&creator, &1);
    client.finalize_raffle();

    env.ledger()
        .set_timestamp(1_000 + EMERGENCY_WITHDRAW_DELAY_SECONDS + 1);

    let result = client.try_emergency_withdraw(&creator);
    assert_eq!(result.err(), Some(Ok(Error::InvalidStatus)));
    let raffle = client.get_raffle();
    assert_eq!(raffle.status, RaffleStatus::Finalized);
    assert!(raffle.prize_deposited);
}

#[test]
fn emergency_withdraw_fails_for_no_deadline_raffle_before_timeout() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let oracle = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = StellarAssetClient::new(&env, &payment_token);
    token_client.mint(&creator, &1_000_000);

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Refund test"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 5,
        max_tickets_per_tx: 5,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: payment_token.clone(),
        prize_amount: MIN_TICKET_PRICE * 5,
        prizes: vec![&env, 10000u32],
        randomness_source: RandomnessSource::External,
        oracle_address: Some(oracle.clone()),
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        unique_winners: false,
        metadata_hash: BytesN::from_array(&env, &[11; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();
    client.buy_tickets(&creator, &1);
    client.finalize_raffle();

    let start_events = env.events().all().len();
    let result = client.try_emergency_withdraw(&creator);
    assert_eq!(env.events().all().len(), start_events);
    assert_eq!(result.err(), Some(Ok(Error::EmergencyTooEarly)));
}

#[test]
fn emergency_withdraw_succeeds_for_drawing_raffle_after_timeout() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let oracle = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = StellarAssetClient::new(&env, &payment_token);
    token_client.mint(&creator, &1_000_000);

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Test"),
        end_time: 2_000,
        no_deadline: false,
        max_tickets: 1,
        max_tickets_per_tx: 1,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: payment_token.clone(),
        prize_amount: MIN_TICKET_PRICE * 10,
        prizes: soroban_sdk::vec![&env, 10000],
        randomness_source: RandomnessSource::External,
        oracle_address: Some(oracle),
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        unique_winners: false,
        metadata_hash: BytesN::from_array(&env, &[12; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();
    client.buy_tickets(&creator, &1);
    client.finalize_raffle();

    env.ledger()
        .set_timestamp(2_000 + EMERGENCY_WITHDRAW_DELAY_SECONDS);

    let result = client.try_emergency_withdraw(&creator);
    assert_eq!(result.err(), Some(Ok(Error::EmergencyTooEarly)));

    env.ledger()
        .set_timestamp(2_000 + EMERGENCY_WITHDRAW_DELAY_SECONDS + 1);
    client.emergency_withdraw(&creator);
    let raffle = client.get_raffle();
    assert_eq!(raffle.status, RaffleStatus::Cancelled);
    assert!(!raffle.prize_deposited);
}

#[test]
fn emergency_withdraw_fails_in_active_state() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = StellarAssetClient::new(&env, &payment_token);
    token_client.mint(&creator, &1_000_000);

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Test"),
        end_time: 10_000,
        no_deadline: false,
        max_tickets: 1,
        max_tickets_per_tx: 1,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: payment_token.clone(),
        prize_amount: MIN_TICKET_PRICE * 10,
        prizes: soroban_sdk::vec![&env, 10000],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        unique_winners: false,
        metadata_hash: BytesN::from_array(&env, &[13; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();

    let start_events = env.events().all().len();
    let result = client.try_emergency_withdraw(&creator);
    assert_eq!(env.events().all().len(), start_events);
    assert_eq!(result.err(), Some(Ok(Error::InvalidStatus)));
}

#[test]
fn emergency_withdraw_fails_in_cancelled_state() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = StellarAssetClient::new(&env, &payment_token);
    token_client.mint(&creator, &1_000_000);

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Test"),
        end_time: 2_000,
        no_deadline: false,
        max_tickets: 1,
        max_tickets_per_tx: 1,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: payment_token.clone(),
        prize_amount: MIN_TICKET_PRICE * 10,
        prizes: soroban_sdk::vec![&env, 10000],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        unique_winners: false,
        metadata_hash: BytesN::from_array(&env, &[14; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();
    client.cancel_raffle(&CancelReason::Other);

    let result = client.try_emergency_withdraw(&creator);
    assert_eq!(result.err(), Some(Ok(Error::InvalidStatus)));
}

#[test]
fn emergency_withdraw_fails_if_prize_not_deposited() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Test"),
        end_time: 10_000,
        no_deadline: false,
        max_tickets: 1,
        max_tickets_per_tx: 1,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: payment_token.clone(),
        prize_amount: MIN_TICKET_PRICE * 10,
        prizes: soroban_sdk::vec![&env, 10000],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        unique_winners: false,
        metadata_hash: BytesN::from_array(&env, &[5; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);

    let start_events = env.events().all().len();
    let result = client.try_emergency_withdraw(&creator);
    assert_eq!(env.events().all().len(), start_events);
    assert_eq!(result.err(), Some(Ok(Error::PrizeNotDeposited)));
}

#[test]
fn emergency_withdraw_only_callable_by_creator_or_admin() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let stranger = Address::generate(&env);
    let oracle = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = StellarAssetClient::new(&env, &payment_token);
    token_client.mint(&creator, &1_000_000);

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Test"),
        end_time: 2_000,
        no_deadline: false,
        max_tickets: 1,
        max_tickets_per_tx: 1,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: payment_token.clone(),
        prize_amount: MIN_TICKET_PRICE * 10,
        prizes: soroban_sdk::vec![&env, 10000],
        randomness_source: RandomnessSource::External,
        oracle_address: Some(oracle),
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        unique_winners: false,
        metadata_hash: BytesN::from_array(&env, &[16; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();
    client.buy_tickets(&creator, &1);
    client.finalize_raffle();

    env.ledger()
        .set_timestamp(1_000 + EMERGENCY_WITHDRAW_DELAY_SECONDS + 1);

    let stranger_result = client.try_emergency_withdraw(&stranger);
    assert_eq!(stranger_result.err(), Some(Ok(Error::NotAuthorized)));

    client.emergency_withdraw(&admin);
}

#[test]
fn emergency_withdraw_sets_status_to_cancelled_and_clears_prize_deposited() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let oracle = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = StellarAssetClient::new(&env, &payment_token);
    token_client.mint(&creator, &1_000_000);

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Guard release"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 5,
        max_tickets_per_tx: 5,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: payment_token.clone(),
        prize_amount: MIN_TICKET_PRICE * 5,
        prizes: soroban_sdk::vec![&env, 10000],
        randomness_source: RandomnessSource::External,
        oracle_address: Some(oracle),
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        unique_winners: false,
        metadata_hash: BytesN::from_array(&env, &[6; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();
    client.buy_tickets(&creator, &1);
    client.finalize_raffle();

    env.ledger()
        .set_timestamp(1_000 + EMERGENCY_WITHDRAW_DELAY_SECONDS + 1);

    client.emergency_withdraw(&creds = &creator);

    let after = client.get_raffle();
    assert_eq!(after.status, RaffleStatus::Cancelled);
    assert!(!after.prize_deposited);
}

#[test]
fn test_refund_guard_released_after_success() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let buyer = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let (payment_token, token_mint) = create_token(&env, &token_admin);
    token_mint.mint(&creator, &1_000_000);
    token_mint.mint(&buyer, &1_000_000);

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Guard release"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 5,
        max_tickets_per_tx: 5,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: payment_token.clone(),
        prize_amount: MIN_TICKET_PRICE * 5,
        prizes: soroban_sdk::vec![&env, 10000],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        unique_winners: false,
        metadata_hash: BytesN::from_array(&env, &[6; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);
    env.as_contract(&contract_id, || {
        env.storage().instance().remove(&DataKey::Factory);
    });

    client.deposit_prize();
    client.buy_tickets(&buyer, &2);
    client.cancel_raffle(&CancelReason::CreatorCancelled);

    client.refund_ticket(&1);
    let second = client.refund_ticket(&2);
    assert_eq!(second, MIN_TICKET_PRICE);

    let guard_set: bool = env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .get(&DataKey::ReentrancyGuard)
            .unwrap_or(false)
    });
    assert!(!guard_set);
}

#[test]
fn test_claim_prize_deducts_protocol_fee() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let buyer = Address::generate(&env);
    let treasury = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = StellarAssetClient::new(&env, &payment_token);
    token_client.mint(&creator, &1_000_000);
    token_client.mint(&buyer, &1_000_000);

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Claim gross"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 1,
        max_tickets_per_tx: 1,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: payment_token.clone(),
        prize_amount: MIN_TICKET_PRICE * 10,
        prizes: soroban_sdk::vec![&env, 10000],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 1_000,
        treasury_address: Some(treasury),
        swap_router: None,
        tikka_token: None,
        unique_winners: false,
        metadata_hash: BytesN::from_array(&env, &[7; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();
    client.buy_tickets(&buyer, &1);
    client.finalize_raffle();

    env.ledger()
        .set_timestamp(1_000 + DEFAULT_CLAIM_LOCKUP_SECONDS + 1);

    let winner = client.get_raffle().winners.get(0).unwrap();
    let balance_before = soroban_sdk::token::Client::new(&env, &payment_token).balance(&winner);

    let gross = MIN_TICKET_PRICE * 10;
    let prize_fee = (gross * 1_000 + 9999) / 10_000;
    let net = gross - prize_fee;

    let claimed = client.claim_prize(&winner, &0);
    assert_eq!(claimed, gross);

    let balance_after = soroban_sdk::token::Client::new(&env, &payment_token).balance(&winner);
    assert_eq!(balance_after, balance_before + net);

    let ticket_fee = (MIN_TICKET_PRICE * 1_000 + 9999) / 10_000;
    assert_eq!(client.get_accumulated_fees(), ticket_fee + prize_fee);
}

#[test]
fn prize_distribution_invariant_holds_for_multiple_tiers() {
    let tier_configs: [[u32; 3]; 3] = [[10000, 0, 0], [5000, 5000, 0], [6000, 3000, 1000]];
    let fee_bps = [0u32, 100, 250, 1000, 2000];

    for tiers_raw in tier_configs {
        let tiers_count = if tiers_raw[2] > 0 {
            3
        } else if tiers_raw[1] > 0 {
            2
        } else {
            1
        };

        for fee_bp in fee_bps {
            let env = Env::default();
            env.mock_all_auths();
            env.ledger().set_timestamp(1_000);

            let factory = Address::generate(&env);
            let admin = Address::generate(&env);
            let creator = Address::generate(&env);
            let treasury = Address::generate(&env);
            let buyer_a = Address::generate(&env);
            let buyer_b = Address::generate(&env);
            let buyer_c = Address::generate(&env);

            let token_admin = Address::generate(&env);
            let payment_token = env
                .register_stellar_asset_contract_v2(token_admin.clone())
                .address();
            let token_client = StellarAssetClient::new(&env, &payment_token);
            token_client.mint(&creator, &10_000_000);
            token_client.mint(&buyer_a, &10_000_000);
            token_client.mint(&buyer_b, &10_000_000);
            token_client.mint(&buyer_c, &10_000_000);

            let contract_id = env.register(RaffleInstance, ());
            let client = RaffleInstanceClient::new(&env, &contract_id);

            let prize_amount: i128 = 1_000_000;
            let ticket_price: i128 = MIN_TICKET_PRICE;
            let tickets_to_sell: u32 = tiers_count;
            let total_ticket_sales = ticket_price * tickets_to_sell as i128;
            let expected_ticket_fees = total_ticket_sales * fee_bp as i128 / 10_000;

            let prizes = match tiers_count {
                1 => soroban_sdk::vec![&env, tiers_raw[0]],
                2 => soroban_sdk::vec![&env, tiers_raw[0], tiers_raw[1]],
                _ => soroban_sdk::vec![&env, tiers_raw[0], tiers_raw[1], tiers_raw[2]],
            };

            let config = RaffleConfig {
                description: String::from_str(&env, "Prize invariant"),
                end_time: 0,
                no_deadline: true,
                max_tickets: tickets_to_sell,
                max_tickets_per_tx: tickets_to_sell,
                min_tickets: 1,
                allow_multiple: true,
                ticket_price,
                payment_token: payment_token.clone(),
                prize_amount,
                prizes,
                randomness_source: RandomnessSource::Internal,
                oracle_address: None,
                protocol_fee_bp: fee_bp,
                treasury_address: Some(treasury.clone()),
                swap_router: None,
                tikka_token: None,
                unique_winners: false,
                metadata_hash: BytesN::from_array(&env, &[33; 32]),
                claim_lockup_seconds: None,
                swap_deadline_seconds: None,
                early_bird_ticket_percentage: 0,
                early_bird_discount_bp: 0,
                category: None,
            };

            client.init(&factory, &admin, &creator, &config);
            client.deposit_prize();

            client.buy_tickets(&buyer_a, &1);
            if tickets_to_sell > 1 {
                client.buy_tickets(&buyer_b, &1);
            }
            if tickets_to_sell > 2 {
                client.buy_tickets(&buyer_c, &1);
            }

            client.finalize_raffle();

            let token = soroban_sdk::token::Client::new(&env, &payment_token);
            let contract_balance_before_claims = token.balance(&contract_id);

            env.ledger()
                .set_timestamp(1_000 + DEFAULT_CLAIM_LOCKUP_SECONDS + 1);

            let raffle = client.get_raffle();
            let winners = raffle.winners;
            let mut total_claimed = 0i128;
            let mut fee_from_prize = 0i128;

            for tier_idx in 0..tiers_count {
                let amt =
                    client.claim_prize(&winners.get(tier_idx as u32).unwrap(), &(tier_idx as u32));
                total_claimed += amt;
                let tier_fee = (amt * fee_bp as i128 + 9999) / 10_000;
                fee_from_prize += tier_fee;
            }

            let leftover = prize_amount - total_claimed;
            assert_eq!(leftover, 0);

            assert_eq!(client.get_accumulated_fees(), expected_ticket_fees + fee_from_prize);
            assert_eq!(token.balance(&treasury), expected_ticket_fees);

            let contract_balance_after_claims = token.balance(&contract_id);
            assert_eq!(
                contract_balance_after_claims,
                contract_balance_before_claims - prize_amount
            );
        }
    }
}

#[test]
fn commit_reveal_entropy_is_mixed_from_all_tickets() {
    fn run_seed(commit_b: [u8; 32], metadata_byte: u8) -> u64 {
        let env = Env::default();
        env.mock_all_auths();

        let factory = Address::generate(&env);
        let admin = Address::generate(&env);
        let creator = Address::generate(&env);
        let buyer_a = Address::generate(&env);
        let buyer_b = Address::generate(&env);
        let buyer_c = Address::generate(&env);

        let token_admin = Address::generate(&env);
        let payment_token = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        let token_client = StellarAssetClient::new(&env, &payment_token);
        token_client.mint(&creator, &1_000_000);
        token_client.mint(&buyer_a, &1_000_000);
        token_client.mint(&buyer_b, &1_000_000);
        token_client.mint(&buyer_c, &1_000_000);

        let contract_id = env.register(RaffleInstance, ());
        let client = RaffleInstanceClient::new(&env, &contract_id);

        let config = RaffleConfig {
            description: String::from_str(&env, "Commit reveal entropy"),
            end_time: 0,
            no_deadline: true,
            max_tickets: 3,
            max_tickets_per_tx: 3,
            min_tickets: 1,
            allow_multiple: true,
            ticket_price: MIN_TICKET_PRICE,
            payment_token: payment_token.clone(),
            prize_amount: MIN_TICKET_PRICE * 10,
            prizes: soroban_sdk::vec![&env, 6000, 3000, 1000],
            randomness_source: RandomnessSource::CommitReveal,
            oracle_address: None,
            protocol_fee_bp: 0,
            treasury_address: None,
            swap_router: None,
            tikka_token: None,
            unique_winners: false,
            metadata_hash: BytesN::from_array(&env, &[metadata_byte; 32]),
            claim_lockup_seconds: None,
            swap_deadline_seconds: None,
            early_bird_ticket_percentage: 0,
            early_bird_discount_bp: 0,
            category: None,
        };

        client.init(&factory, &admin, &creator, &config);
        client.deposit_prize();
        client.buy_tickets(&buyer_a, &1);
        client.buy_tickets(&buyer_b, &1);
        client.buy_tickets(&buyer_c, &1);

        let commit_a = [1u8; 32];
        let commit_c = [3u8; 32];
        client.submit_commit(&1, &BytesN::from_array(&env, &commit_a));
        client.submit_commit(&2, &BytesN::from_array(&env, &commit_b));
        client.submit_commit(&3, &BytesN::from_array(&env, &commit_c));

        client.finalize_raffle();

        let fairness = client.get_fairness_data();

        let mut combined = Bytes::new(&env);
        combined.extend_from_array(&commit_a);
        combined.extend_from_array(&commit_b);
        combined.extend_from_array(&commit_c);
        let hash: BytesN<32> = env.crypto().sha256(&combined).into();
        let arr = hash.to_array();
        let expected_seed = u64::from_be_bytes([
            arr[0], arr[1], arr[2], arr[3], arr[4], arr[5], arr[6], arr[7],
        ]);

        assert_eq!(fairness.seed, expected_seed);
        fairness.seed
    }

    let seed_original = run_seed([2u8; 32], 44);
    let seed_changed = run_seed([9u8; 32], 45);
    assert_ne!(seed_original, seed_changed);
}

#[test]
fn commit_reveal_preserves_entropy_after_ticket_transfer() {
    let env = Env::default();
    env.mock_all_auths();

    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let buyer_a = Address::generate(&env);
    let buyer_b = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = StellarAssetClient::new(&env, &payment_token);
    token_client.mint(&creator, &1_000_000);
    token_client.mint(&buyer_a, &1_000_000);
    token_client.mint(&buyer_b, &1_000_000);

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Commit survives transfer"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 1,
        max_tickets_per_tx: 1,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: payment_token.clone(),
        prize_amount: MIN_TICKET_PRICE * 5,
        prizes: soroban_sdk::vec![&env, 10000],
        randomness_source: RandomnessSource::CommitReveal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        unique_winners: false,
        metadata_hash: BytesN::from_array(&env, &[46; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();
    client.buy_tickets(&buyer_a, &1);

    let commit = [7u8; 32];
    client.submit_commit(&1, &BytesN::from_array(&env, &commit));

    env.as_contract(&contract_id, || {
        let mut ticket: Ticket = env.storage().persistent().get(&DataKey::Ticket(1)).unwrap();
        ticket.owner = buyer_b.clone();
        env.storage().persistent().set(&DataKey::Ticket(1), &ticket);
    });

    client.finalize_raffle();
    let fairness = client.get_fairness_data();

    let mut combined = Bytes::new(&env);
    combined.extend_from_array(&commit);
    let hash: BytesN<32> = env.crypto().sha256(&combined).into();
    let arr = hash.to_array();
    let expected_seed = u64::from_be_bytes([
        arr[0], arr[1], arr[2], arr[3], arr[4], arr[5], arr[6], arr[7],
    ]);

    assert_eq!(fairness.seed, expected_seed);
}

#[test]
fn commit_reveal_with_zero_commits_falls_back_to_prng() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(7_777);
    env.ledger().with_mut(|l| {
        l.sequence_number = 999;
    });

    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let buyer_a = Address::generate(&env);
    let buyer_b = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = StellarAssetClient::new(&env, &payment_token);
    token_client.mint(&creator, &1_000_000);
    token_client.mint(&buyer_a, &1_000_000);
    token_client.mint(&buyer_b, &1_000_000);

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Commit reveal no commits"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 2,
        max_tickets_per_tx: 2,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: payment_token.clone(),
        prize_amount: MIN_TICKET_PRICE * 8,
        prizes: soroban_sdk::vec![&env, 7000, 3000],
        randomness_source: RandomnessSource::CommitReveal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        unique_winners: false,
        metadata_hash: BytesN::from_array(&env, &[47; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();
    client.buy_tickets(&buyer_a, &1);
    client.buy_tickets(&buyer_b, &1);
    client.finalize_raffle();

    let raffle = client.get_raffle();
    assert_eq!(raffle.status, RaffleStatus::Finalized);

    let fairness = client.get_fairness_data();
    let expected_seed = env.as_contract(&contract_id, || {
        let payload = (
            env.ledger().timestamp(),
            env.ledger().sequence(),
            env.current_contract_address().to_xdr(&env),
        )
            .to_xdr(&env);
        let hash: BytesN<32> = env.crypto().sha256(&payload).into();
        let arr = hash.to_array();
        u64::from_be_bytes([
            arr[0], arr[1], arr[2], arr[3], arr[4], arr[5], arr[6], arr[7],
        ])
    });
    assert_eq!(fairness.seed, expected_seed);
    assert_eq!(raffle.winners.len(), 2);
}

#[test]
fn drawing_lock_cleared_after_internal_finalize() {
    let env = Env::default();
    env.mock_all_auths();

    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = StellarAssetClient::new(&env, &payment_token);
    token_client.mint(&creator, &1_000_000);

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Lock internal finalize"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 1,
        max_tickets_per_tx: 1,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: payment_token.clone(),
        prize_amount: MIN_TICKET_PRICE * 2,
        prizes: soroban_sdk::vec![&env, 10000],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        unique_winners: false,
        metadata_hash: BytesN::from_array(&env, &[48; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();
    client.buy_tickets(&creator, &1);
    client.finalize_raffle();

    assert_drawing_lock_cleared(&env, &contract_id);
}

#[test]
fn drawing_lock_cleared_after_oracle_seed_delivered() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let oracle = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = StellarAssetClient::new(&env, &payment_token);
    token_client.mint(&creator, &1_000_000);

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Lock oracle finalize"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 1,
        max_tickets_per_tx: 1,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: payment_token.clone(),
        prize_amount: MIN_TICKET_PRICE * 2,
        prizes: soroban_sdk::vec![&env, 10000],
        randomness_source: RandomnessSource::External,
        oracle_address: Some(oracle),
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        unique_winners: false,
        metadata_hash: BytesN::from_array(&env, &[49; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();
    client.buy_tickets(&creator, &1);

    let request_id: u64 = env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .get(&DataKey::RandomnessRequestId)
            .unwrap()
    });

    let signing_key = SigningKey::from_bytes(&[5u8; 32]);
    let verifying = signing_key.verifying_key();
    let message = env.as_contract(&contract_id, || {
        build_vrf_proof_message(&env, request_id, 424242)
    });
    let signature = signing_key.sign(message.as_slice());

    client.provide_randomness(
        &424242,
        &BytesN::from_array(&env, &verifying.to_bytes()),
        &BytesN::from_array(&env, &signature.to_bytes()),
        &request_id,
    );

    assert_drawing_lock_cleared(&env, &contract_id);
}

#[test]
fn drawing_lock_cleared_after_fallback_finalize() {
    let env = Env::default();
    env.mock_all_auths();

    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let oracle = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = StellarAssetClient::new(&env, &payment_token);
    token_client.mint(&creator, &1_000_000);

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Lock fallback finalize"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 1,
        max_tickets_per_tx: 1,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: payment_token.clone(),
        prize_amount: MIN_TICKET_PRICE * 2,
        prizes: soroban_sdk::vec![&env, 10000],
        randomness_source: RandomnessSource::External,
        oracle_address: Some(oracle),
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        unique_winners: false,
        metadata_hash: BytesN::from_array(&env, &[51; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();
    client.buy_tickets(&creator, &1);

    env.ledger().with_mut(|l| {
        l.sequence_number += ORACLE_TIMEOUT_LEDGERS + 1;
    });
    client.trigger_randomness_fallback(&creator, &false);

    assert_drawing_lock_cleared(&env, &contract_id);
}

#[test]
fn drawing_lock_cleared_after_cancel_in_drawing_state() {
    let env = Env::default();
    env.mock_all_auths();

    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let oracle = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let token_client = StellarAssetClient::new(&env, &payment_token);
    token_client.mint(&creator, &1_000_000);

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Cancel drawing"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 10,
        max_tickets_per_tx: 10,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: payment_token.clone(),
        prize_amount: MIN_TICKET_PRICE * 2,
        prizes: soroban_sdk::vec![&env, 10000],
        randomness_source: RandomnessSource::External,
        oracle_address: Some(oracle),
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        unique_winners: false,
        metadata_hash: BytesN::from_array(&env, &[52; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();
    client.buy_tickets(&creator, &1);
    client.cancel_raffle(&CancelReason::CreatorCancelled);

    assert_drawing_lock_cleared(&env, &contract_id);
}

#[test]
fn test_bundle_pricing_applies() {
    let env = Env::default();
    env.mock_all_auths();

    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let buyer = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = StellarAssetClient::new(&env, &payment_token);
    token_client.mint(&creator, &100_000_000);
    token_client.mint(&buyer, &100_000_000);

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Bundle test"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 50,
        max_tickets_per_tx: 50,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: 100_000,
        payment_token: payment_token.clone(),
        prize_amount: 100_000 * 50,
        prizes: soroban_sdk::vec![&env, 10000],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        unique_winners: false,
        metadata_hash: BytesN::from_array(&env, &[9; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);
    env.as_contract(&contract_id, || {
        env.storage().instance().remove(&DataKey::Factory);
    });

    client.deposit_prize();

    let balance_before = token_client.balance(&buyer);
    client.buy_tickets(&buyer, &11);
    let balance_after = token_client.balance(&buyer);

    assert_eq!(balance_before - balance_after, 11 * 80_000);
}

fn lifecycle_config(env: &Env, payment_token: &Address, treasury: &Address) -> RaffleConfig {
    RaffleConfig {
        description: String::from_str(env, "Full lifecycle"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 3,
        max_tickets_per_tx: 3,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: payment_token.clone(),
        prize_amount: MIN_TICKET_PRICE * 100,
        prizes: soroban_sdk::vec![env, 10000],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 100,
        treasury_address: Some(treasury.clone()),
        swap_router: None,
        tikka_token: None,
        unique_winners: false,
        metadata_hash: BytesN::from_array(env, &[70u8; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    }
}

fn init_bounds_config(
    env: &Env,
    payment_token: &Address,
    description: String,
    max_tickets: u32,
    ticket_price: i128,
    prize_amount: i128,
    prizes: soroban_sdk::Vec<u32>,
) -> RaffleConfig {
    RaffleConfig {
        description,
        end_time: 0,
        no_deadline: true,
        max_tickets,
        max_tickets_per_tx: max_tickets,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price,
        payment_token: payment_token.clone(),
        prize_amount,
        prizes,
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        unique_winners: false,
        metadata_hash: BytesN::from_array(env, &[72u8; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    }
}

fn assert_metadata_hash(client: &RaffleInstanceClient<'_>, expected: &BytesN<32>) {
    let raffle = client.get_raffle();
    assert_eq!(&raffle.metadata_hash, expected);
}

fn init_bounds_env() -> (Env, Address, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let contract_id = env.register(RaffleInstance, ());
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let factory = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    (env, contract_id, factory, admin, creator, payment_token)
}

#[test]
fn init_accepts_min_ticket_price_and_rejects_below_it() {
    let (env, contract_id, factory, admin, creator, payment_token) = init_bounds_env();
    let client = RaffleInstanceClient::new(&env, &contract_id);
    let prizes = soroban_sdk::vec![&env, 10000u32];

    let config = init_bounds_config(
        &env,
        &payment_token,
        String::from_str(&env, "min ticket price"),
        5,
        MIN_TICKET_PRICE,
        MIN_TICKET_PRICE * 5,
        prizes.clone(),
    );
    client.init(&factory, &admin, &creator, &config);

    let invalid = init_bounds_config(
        &env,
        &payment_token,
        String::from_str(&env, "min ticket price"),
        5,
        MIN_TICKET_PRICE - 1,
        MIN_TICKET_PRICE * 5,
        prizes,
    );
    assert_eq!(
        client.try_init(&factory, &admin, &creator, &invalid),
        Err(Ok(Error::InvalidParameters))
    );
}

#[test]
fn init_accepts_max_prize_amount_and_rejects_above_it() {
    let (env, contract_id, factory, admin, creator, payment_token) = init_bounds_env();
    let client = RaffleInstanceClient::new(&env, &contract_id);
    let prizes = soroban_sdk::vec![&env, 10000u32];

    let config = init_bounds_config(
        &env,
        &payment_token,
        String::from_str(&env, "max prize amount"),
        5,
        MIN_TICKET_PRICE,
        MAX_PRIZE_AMOUNT,
        prizes.clone(),
    );
    client.init(&factory, &admin, &creator, &config);

    let invalid = init_bounds_config(
        &env,
        &payment_token,
        String::from_str(&env, "max prize amount"),
        5,
        MIN_TICKET_PRICE,
        MAX_PRIZE_AMOUNT + 1,
        prizes,
    );
    assert_eq!(
        client.try_init(&factory, &admin, &creator, &invalid),
        Err(Ok(Error::InvalidParameters))
    );
}

#[test]
fn init_accepts_max_description_length_and_rejects_above_it() {
    let (env, contract_id, factory, admin, creator, payment_token) = init_bounds_env();
    let client = RaffleInstanceClient::new(&env, &contract_id);
    let prizes = soroban_sdk::vec![&env, 10000u32];
    let inside_description = String::from_str(&env, &"a".repeat(MAX_DESCRIPTION_LENGTH as usize));
    let outside_description =
        String::from_str(&env, &"a".repeat(MAX_DESCRIPTION_LENGTH as usize + 1));

    let config = init_bounds_config(
        &env,
        &payment_token,
        inside_description,
        5,
        MIN_TICKET_PRICE,
        MIN_TICKET_PRICE * 5,
        prizes.clone(),
    );
    client.init(&factory, &admin, &creator, &config);

    let invalid = init_bounds_config(
        &env,
        &payment_token,
        outside_description,
        5,
        MIN_TICKET_PRICE,
        MIN_TICKET_PRICE * 5,
        prizes,
    );
    assert_eq!(
        client.try_init(&factory, &admin, &creator, &invalid),
        Err(Ok(Error::InvalidParameters))
    );
}

#[test]
fn init_accepts_max_prizes_and_rejects_above_it() {
    let (env, contract_id, factory, admin, creator, payment_token) = init_bounds_env();
    let client = RaffleInstanceClient::new(&env, &contract_id);
    let mut inside_prizes = soroban_sdk::Vec::new(&env);
    for _ in 0..MAX_PRIZES {
        inside_prizes.push_back(10000u32);
    }
    let mut outside_prizes = soroban_sdk::Vec::new(&env);
    for _ in 0..(MAX_PRIZES + 1) {
        outside_prizes.push_back(10000u32);
    }

    let config = init_bounds_config(
        &env,
        &payment_token,
        String::from_str(&env, "max prizes"),
        5,
        MIN_TICKET_PRICE,
        MIN_TICKET_PRICE * 5,
        inside_prizes,
    );
    client.init(&factory, &admin, &creator, &config);

    let invalid = init_bounds_config(
        &env,
        &payment_token,
        String::from_str(&env, "max prizes"),
        5,
        MIN_TICKET_PRICE,
        MIN_TICKET_PRICE * 5,
        outside_prizes,
    );
    assert_eq!(
        client.try_init(&factory, &admin, &creator, &invalid),
        Err(Ok(Error::TooManyPrizes))
    );
}

#[test]
fn init_accepts_max_tickets_limit_and_rejects_above_it() {
    let (env, contract_id, factory, admin, creator, payment_token) = init_bounds_env();
    let client = RaffleInstanceClient::new(&env, &contract_id);
    let prizes = soroban_sdk::vec![&env, 10000u32];

    let config = init_bounds_config(
        &env,
        &payment_token,
        String::from_str(&env, "max tickets"),
        MAX_TICKETS_LIMIT,
        MIN_TICKET_PRICE,
        MIN_TICKET_PRICE * 5,
        prizes.clone(),
    );
    client.init(&factory, &admin, &creator, &config);

    let invalid = init_bounds_config(
        &env,
        &payment_token,
        String::from_str(&env, "max tickets"),
        MAX_TICKETS_LIMIT + 1,
        MIN_TICKET_PRICE,
        MIN_TICKET_PRICE * 5,
        prizes,
    );
    assert_eq!(
        client.try_init(&factory, &admin, &creator, &invalid),
        Err(Ok(Error::InvalidParameters))
    );
}

#[test]
fn update_metadata_hash_before_deposit_only() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(&env, &contract_id);
    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_addr = sac.address();

    let config = RaffleConfig {
        description: soroban_sdk::String::from_str(&env, "metadata"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 5,
        max_tickets_per_tx: 5,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: 10_000,
        payment_token: token_addr,
        prize_amount: 50_000,
        prizes: soroban_sdk::vec![&env, 10_000u32],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        unique_winners: false,
        metadata_hash: BytesN::from_array(&env, &[1u8; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);
    let new_hash = BytesN::from_array(&env, &[2u8; 32]);
    client.update_metadata_hash(&new_hash);
    assert_eq!(client.get_raffle().metadata_hash, new_hash);

    token::StellarAssetClient::new(&env, &token_addr).mint(&creator, &1_000_000);
    client.deposit_prize();
    assert_eq!(
        client.try_update_metadata_hash(&BytesN::from_array(&env, &[3u8; 32])),
        Err(Ok(Error::InvalidStatus))
    );
}

#[test]
fn test_adversarial_ceiling_rounding() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let buyer = Address::generate(&env);
    let treasury = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = StellarAssetClient::new(&env, &payment_token);

    let ticket_price = 10_001i128;
    token_client.mint(&creator, &1_000_000);
    token_client.mint(&buyer, &1_000_000);

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Adversarial Rounding"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 1,
        max_tickets_per_tx: 1,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price,
        payment_token: payment_token.clone(),
        prize_amount: ticket_price,
        prizes: soroban_sdk::vec![&env, 10000],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 1,
        treasury_address: Some(treasury),
        swap_router: None,
        tikka_token: None,
        unique_winners: false,
        metadata_hash: BytesN::from_array(&env, &[9; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();
    client.buy_tickets(&buyer, &1);

    let expected_ticket_fee = 2i128;
    assert_eq!(client.get_accumulated_fees(), expected_ticket_fee);

    client.finalize_raffle();
    let winner = client.get_raffle().winners.get(0).unwrap();
    let balance_before = token_client.balance(&winner);

    let claimed = client.claim_prize(&winner, &0);
    assert_eq!(claimed, ticket_price);

    let expected_prize_fee = 2i128;
    assert_eq!(client.get_accumulated_fees(), expected_ticket_fee + expected_prize_fee);

    let balance_after = token_client.balance(&winner);
    assert_eq!(balance_after, balance_before + ticket_price - expected_prize_fee);
}

#[test]
fn test_explicit_zero_lockup_is_honored() {
    let env = Env::default();
    env.mock_all_auths();
    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Zero Lockup"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 1,
        max_tickets_per_tx: 1,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: 1,
        payment_token: payment_token.clone(),
        prize_amount: 1,
        prizes: soroban_sdk::vec![&env, 10000],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        unique_winners: false,
        metadata_hash: BytesN::from_array(&env, &[99; 32]),
        claim_lockup_seconds: Some(0),
        swap_deadline_seconds: Some(0),
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);
    let raffle = client.get_raffle();

    assert_eq!(raffle.claim_lockup_seconds, 0);
    assert_eq!(raffle.swap_deadline_seconds, 0);
}

#[test]
fn test_unset_lockup_gets_default() {
    let env = Env::default();
    env.mock_all_auths();
    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Default Lockup"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 1,
        max_tickets_per_tx: 1,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: 1,
        payment_token: payment_token.clone(),
        prize_amount: 1,
        prizes: soroban_sdk::vec![&env, 10000],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        unique_winners: false,
        metadata_hash: BytesN::from_array(&env, &[100; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);
    let raffle = client.get_raffle();

    assert_eq!(raffle.claim_lockup_seconds, DEFAULT_CLAIM_LOCKUP_SECONDS);
    assert_eq!(raffle.swap_deadline_seconds, DEFAULT_SWAP_DEADLINE_SECONDS);
}

#[test]
fn unique_winners_limits_one_tier_per_address() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(&env, &contract_id);
    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let buyer_a = Address::generate(&env);
    let buyer_b = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_addr = sac.address();
    token::StellarAssetClient::new(&env, &token_addr).mint(&creator, &10_000_000);
    token::StellarAssetClient::new(&env, &token_addr).mint(&buyer_a, &10_000_000);
    token::StellarAssetClient::new(&env, &token_addr).mint(&buyer_b, &10_000_000);

    let prizes = soroban_sdk::vec![&env, 7000u32, 3000u32];

    let config = RaffleConfig {
        description: soroban_sdk::String::from_str(&env, "unique winners"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 10,
        max_tickets_per_tx: 5,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: 10_000,
        payment_token: token_addr.clone(),
        prize_amount: 100_000,
        prizes,
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        unique_winners: true,
        metadata_hash: BytesN::from_array(&env, &[42u8; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();
    client.buy_tickets(&buyer_a, &5);
    client.buy_tickets(&buyer_b, &5);

    env.ledger().set_timestamp(5_000);
    client.finalize_raffle();

    let raffle = client.get_raffle();
    assert_eq!(raffle.winners.len(), 2);

    let mut count_a = 0u32;
    let mut count_b = 0u32;
    for i in 0..raffle.winners.len() {
        let w = raffle.winners.get(i).unwrap();
        if w == buyer_a {
            count_a += 1;
        }
        if w == buyer_b {
            count_b += 1;
        }
    }
    assert_eq!(count_a, 1);
    assert_eq!(count_b, 1);

    let fairness = client.get_fairness_data();
    assert!(fairness.unique_winners);
}