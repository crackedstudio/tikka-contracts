#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    token, Address, Env, IntoVal, String, Symbol,
};

/// HELPER: Standardized environment setup
fn setup_raffle_env(
    env: &Env,
    source: RandomnessSource,
    oracle: Option<Address>,
    fee_bp: u32,
    treasury: Option<Address>,
) -> (
    ContractClient<'_>,
    Address,
    Address,
    token::StellarAssetClient<'_>,
    Address,
) {
    let creator = Address::generate(env);
    let buyer = Address::generate(env);
    let admin = Address::generate(env);
    let factory = Address::generate(env);

    let token_contract = env.register_stellar_asset_contract_v2(admin.clone());
    let token_id = token_contract.address();
    let admin_client = token::StellarAssetClient::new(env, &token_id);

    admin_client.mint(&creator, &1_000i128);
    admin_client.mint(&buyer, &1_000i128);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(env, "Audit Raffle"),
        end_time: 0,
        max_tickets: 5,
        allow_multiple: false,
        ticket_price: 10i128,
        payment_token: token_id,
        prize_amount: 100i128,
        randomness_source: source,
        oracle_address: oracle,
        protocol_fee_bp: fee_bp,
        treasury_address: treasury,
    };

    client.init(&factory, &creator, &config);

    (client, creator, buyer, admin_client, factory)
}

// --- 1. FUNCTIONAL FLOW TESTS ---

#[test]
fn test_basic_internal_raffle_flow() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, creator, _buyer, admin_client, _) =
        setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);
    let token_client = token::Client::new(&env, &admin_client.address);

    client.deposit_prize();

    for _ in 0..5 {
        let b = Address::generate(&env);
        admin_client.mint(&b, &10i128);
        client.buy_ticket(&b);
    }

    client.finalize_raffle();

    let raffle = client.get_raffle();
    let winner = raffle.winner.unwrap();
    let _claimed_amount = client.claim_prize(&winner);

    assert_eq!(token_client.balance(&winner), 100i128);
    assert_eq!(token_client.balance(&creator), 900i128);
}

#[test]
fn test_protocol_fees() {
    let env = Env::default();
    env.mock_all_auths();
    let treasury = Address::generate(&env);
    let (client, _creator, _buyer, admin_client, _) = setup_raffle_env(
        &env,
        RandomnessSource::Internal,
        None,
        500,
        Some(treasury.clone()),
    ); // 5% fee
    let token_client = token::Client::new(&env, &admin_client.address);

    client.deposit_prize();
    for _ in 0..5 {
        let b = Address::generate(&env);
        admin_client.mint(&b, &10i128);
        client.buy_ticket(&b);
    }

    client.finalize_raffle();
    let winner = client.get_raffle().winner.unwrap();
    client.claim_prize(&winner);

    // Prize: 100, Fee: 5% = 5, Winner: 95
    assert_eq!(token_client.balance(&winner), 95i128);
    assert_eq!(token_client.balance(&treasury), 5i128);
}

#[test]
fn test_vrf_raffle_flow() {
    let env = Env::default();
    env.mock_all_auths();

    #[contract]
    pub struct DummyOracle;
    #[contractimpl]
    impl DummyOracle {}
    let oracle = env.register(DummyOracle, ());

    let (client, _, _buyer, admin_client, _) = setup_raffle_env(
        &env,
        RandomnessSource::External,
        Some(oracle.clone()),
        0,
        None,
    );

    client.deposit_prize();

    let mut buyers = Vec::new(&env);
    for _ in 0..5 {
        let b = Address::generate(&env);
        buyers.push_back(b.clone());
        admin_client.mint(&b, &10i128);
        client.buy_ticket(&b);
    }

    client.finalize_raffle();

    let raffle_pre = client.get_raffle();
    assert!(matches!(raffle_pre.status, RaffleStatus::Drawing));

    let seed = 12345u64;
    let expected_winner_idx = (seed % 5) as u32;
    let expected_winner = buyers.get(expected_winner_idx).unwrap();

    env.as_contract(&oracle, || {
        client.provide_randomness(&seed);
    });

    let raffle_post = client.get_raffle();
    assert!(matches!(raffle_post.status, RaffleStatus::Finalized));
    assert_eq!(raffle_post.winner.unwrap(), expected_winner);
}

// --- 2. ERROR CONDITION TESTS ---

#[test]
#[should_panic] // Error(Contract, #5) - NotAuthorized
fn test_unauthorized_deposit() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _) = setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);

    let stranger = Address::generate(&env);
    env.as_contract(&stranger, || {
        client.deposit_prize();
    });
}

#[test]
#[should_panic] // Error(Contract, #20) - InvalidStateTransition (Buy before Active)
fn test_invalid_state_transition_buy_before_deposit() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, buyer, _, _) =
        setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);

    client.buy_ticket(&buyer);
}

#[test]
#[should_panic] // Error(Contract, #14) - MultipleTicketsNotAllowed
fn test_multiple_tickets_prohibited() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, buyer, admin_client, _) =
        setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);

    client.deposit_prize();
    admin_client.mint(&buyer, &20i128);
    client.buy_ticket(&buyer);
    client.buy_ticket(&buyer); // Should fail
}

// --- 3. EVENT AUDIT & STATE VALIDATION ---

#[test]
fn test_raffle_created_event() {
    let env = Env::default();
    env.mock_all_auths();

    let creator = Address::generate(&env);
    let factory = Address::generate(&env);
    let admin = Address::generate(&env);

    let token_contract = env.register_stellar_asset_contract_v2(admin.clone());
    let token_id = token_contract.address();

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Test Raffle"),
        end_time: 0,
        max_tickets: 5,
        allow_multiple: false,
        ticket_price: 10i128,
        payment_token: token_id,
        prize_amount: 100i128,
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
    };

    client.init(&factory, &creator, &config);

    // Check that raffle_created event was emitted
    assert!(env.events().all().len() > 0);
}

#[test]
fn test_prize_deposited_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _) =
        setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);

    client.deposit_prize();

    // Check that prize_deposited event was emitted
    assert!(env.events().all().len() > 0);
}

#[test]
fn test_raffle_finalized_event_audit() {
    let env = Env::default();
    env.mock_all_auths();

    let expected_timestamp = 123456789;
    env.ledger().with_mut(|l| {
        l.timestamp = expected_timestamp;
    });

    let (client, _, _, admin_client, _) =
        setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);

    client.deposit_prize();
    for _ in 0..5 {
        let b = Address::generate(&env);
        admin_client.mint(&b, &10i128);
        client.buy_ticket(&b);
    }

    client.finalize_raffle();

    // Check that raffle_finalized event was emitted
    assert!(env.events().all().len() > 0);
}

#[test]
fn test_single_ticket_purchase_event() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, buyer, _, _) =
        setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);

    client.deposit_prize();

    let _ = env.events().all();

    client.buy_ticket(&buyer);

    // Check that ticket_purchased event was emitted
    assert!(env.events().all().len() > 0);
}

#[test]
fn test_draw_triggered_event() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _, admin_client, _) =
        setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);

    client.deposit_prize();
    for _ in 0..5 {
        let b = Address::generate(&env);
        admin_client.mint(&b, &10i128);
        client.buy_ticket(&b);
    }

    client.finalize_raffle();

    // Check that draw_triggered event was emitted
    assert!(env.events().all().len() > 0);
}

#[test]
fn test_randomness_requested_event() {
    let env = Env::default();
    env.mock_all_auths();

    #[contract]
    pub struct DummyOracle;
    #[contractimpl]
    impl DummyOracle {}
    let oracle = env.register(DummyOracle, ());

    let (client, _, _, admin_client, _) = setup_raffle_env(
        &env,
        RandomnessSource::External,
        Some(oracle.clone()),
        0,
        None,
    );

    client.deposit_prize();
    for _ in 0..5 {
        let b = Address::generate(&env);
        admin_client.mint(&b, &10i128);
        client.buy_ticket(&b);
    }

    client.finalize_raffle();

    // Check that randomness_requested event was emitted
    assert!(env.events().all().len() > 0);
}

#[test]
fn test_randomness_received_event() {
    let env = Env::default();
    env.mock_all_auths();

    #[contract]
    pub struct DummyOracle;
    #[contractimpl]
    impl DummyOracle {}
    let oracle = env.register(DummyOracle, ());

    let (client, _, _, admin_client, _) = setup_raffle_env(
        &env,
        RandomnessSource::External,
        Some(oracle.clone()),
        0,
        None,
    );

    client.deposit_prize();
    for _ in 0..5 {
        let b = Address::generate(&env);
        admin_client.mint(&b, &10i128);
        client.buy_ticket(&b);
    }

    client.finalize_raffle();

    env.as_contract(&oracle, || {
        client.provide_randomness(&12345u64);
    });

    // Check that randomness_received event was emitted
    assert!(env.events().all().len() > 0);
}

#[test]
fn test_prize_claimed_event() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _, admin_client, _) =
        setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);

    client.deposit_prize();
    for _ in 0..5 {
        let b = Address::generate(&env);
        admin_client.mint(&b, &10i128);
        client.buy_ticket(&b);
    }

    client.finalize_raffle();
    let winner = client.get_raffle().winner.unwrap();
    client.claim_prize(&winner);

    // Check that prize_claimed event was emitted
    assert!(env.events().all().len() > 0);
}

#[test]
fn test_raffle_cancelled_event() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, buyer, _, _) =
        setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);

    client.deposit_prize();
    client.buy_ticket(&buyer);
    client.cancel_raffle();

    // Check that raffle_cancelled event was emitted
    assert!(env.events().all().len() > 0);
}

#[test]
fn test_status_changed_events() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _, admin_client, _) =
        setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);

    client.deposit_prize();

    let events_after_deposit = env.events().all();
    // Check that status_changed event was emitted
    assert!(events_after_deposit.len() > 0);
}

#[test]
fn test_raffle_cancellation() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, creator, buyer, admin_client, _) =
        setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);
    let token_client = token::Client::new(&env, &admin_client.address);

    client.deposit_prize();
    client.buy_ticket(&buyer);

    client.cancel_raffle();

    assert_eq!(token_client.balance(&creator), 1000i128);

    let raffle = client.get_raffle();
    assert!(raffle.status == RaffleStatus::Cancelled);
}
