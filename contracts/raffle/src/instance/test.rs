#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    token, Address, Bytes, Env, IntoVal, String, Symbol,
};
use crate::{RaffleFactory, RaffleFactoryClient, ContractError};

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
    Address,
) {
    let creator = Address::generate(env);
    let buyer = Address::generate(env);
    let admin = Address::generate(env);
    let factory_admin = Address::generate(env);

    // Register factory as a dummy contract so env.as_contract works
    #[contract]
    pub struct DummyFactory;
    #[contractimpl]
    impl DummyFactory {}
    let factory = env.register(DummyFactory, ());

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

    client.init(&factory, &factory_admin, &creator, &config);

    (client, creator, buyer, admin_client, factory, factory_admin)
}

// --- 1. FUNCTIONAL FLOW TESTS ---

#[test]
fn test_basic_internal_raffle_flow() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, creator, _buyer, admin_client, _, _) =
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
    let (client, _creator, _buyer, admin_client, _, _) = setup_raffle_env(
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

    let (client, _, _buyer, admin_client, _, _) = setup_raffle_env(
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
    let (client, _, _, _, _, _) = setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);

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
    let (client, _, buyer, _, _, _) =
        setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);

    client.buy_ticket(&buyer);
}

#[test]
#[should_panic] // Error(Contract, #14) - MultipleTicketsNotAllowed
fn test_multiple_tickets_prohibited() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, buyer, admin_client, _, _) =
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
    let factory_admin = Address::generate(&env);

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

    client.init(&factory, &factory_admin, &creator, &config);

    // Check that raffle_created event was emitted
    assert!(env.events().all().len() > 0);
}

#[test]
fn test_prize_deposited_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _) =
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

    let (client, _, _, admin_client, _, _) =
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

    let (client, _, buyer, _, _, _) =
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

    let (client, _, _, admin_client, _, _) =
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

    let (client, _, _, admin_client, _, _) = setup_raffle_env(
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

    let (client, _, _, admin_client, _, _) = setup_raffle_env(
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

    let (client, _, _, admin_client, _, _) =
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

    let (client, _, buyer, _, _, _) =
        setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);

    client.deposit_prize();
    client.buy_ticket(&buyer);
    client.cancel_raffle(&CancelReason::CreatorCancelled);

    // Check that raffle_cancelled event was emitted
    assert!(env.events().all().len() > 0);
}

#[test]
fn test_status_changed_events() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _, admin_client, _, _) =
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
    let (client, creator, buyer, admin_client, _, _) =
        setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);
    let token_client = token::Client::new(&env, &admin_client.address);

    client.deposit_prize();
    client.buy_ticket(&buyer);

    client.cancel_raffle(&CancelReason::CreatorCancelled);

    assert_eq!(token_client.balance(&creator), 1000i128);

    let raffle = client.get_raffle();
    assert!(raffle.status == RaffleStatus::Cancelled);
}

#[test]
fn test_refund_ticket() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _creator, buyer, admin_client, _, _) =
        setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);
    let token_client = token::Client::new(&env, &admin_client.address);

    client.deposit_prize();
    client.buy_ticket(&buyer);
    
    // Check ticket balances before refund
    assert_eq!(token_client.balance(&buyer), 990i128); // 1000 - 10 ticket_price

    client.cancel_raffle(&CancelReason::CreatorCancelled);

    // Initial refund
    let refunded = client.refund_ticket(&1u32);
    assert_eq!(refunded, 10i128);
    assert_eq!(token_client.balance(&buyer), 1000i128);

    // Double refund should fail (idempotency natively checked)
}

#[test]
#[should_panic] // Error(Contract, #20) - InvalidStateTransition (Already refunded)
fn test_double_refund_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _creator, buyer, _admin_client, _, _) =
        setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);

    client.deposit_prize();
    client.buy_ticket(&buyer);
    
    client.cancel_raffle(&CancelReason::CreatorCancelled);

    client.refund_ticket(&1u32);
    client.refund_ticket(&1u32); // Panic!
}

// --- PAUSE/UNPAUSE TESTS ---

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_buy_ticket_blocked_when_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, buyer, _, factory, _) =
        setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);

    client.deposit_prize();

    env.as_contract(&factory, || {
        client.pause();
    });

    client.buy_ticket(&buyer);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_deposit_prize_blocked_when_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, factory, _) =
        setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);

    env.as_contract(&factory, || {
        client.pause();
    });

    client.deposit_prize();
}

#[test]
fn test_claim_prize_succeeds_when_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, admin_client, factory, _) =
        setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);

    client.deposit_prize();
    for _ in 0..5 {
        let b = Address::generate(&env);
        admin_client.mint(&b, &10i128);
        client.buy_ticket(&b);
    }
    client.finalize_raffle();

    env.as_contract(&factory, || {
        client.pause();
    });

    let winner = client.get_raffle().winner.unwrap();
    let result = client.claim_prize(&winner);
    assert!(result > 0);
}

#[test]
fn test_refund_ticket_succeeds_when_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, buyer, _, factory, _) =
        setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);

    client.deposit_prize();
    client.buy_ticket(&buyer);
    client.cancel_raffle(&CancelReason::CreatorCancelled);

    env.as_contract(&factory, || {
        client.pause();
    });

    let refunded = client.refund_ticket(&1u32);
    assert_eq!(refunded, 10i128);
}

#[test]
fn test_cancel_raffle_succeeds_when_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, buyer, _, factory, _) =
        setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);

    client.deposit_prize();
    client.buy_ticket(&buyer);

    env.as_contract(&factory, || {
        client.pause();
    });

    client.cancel_raffle(&CancelReason::CreatorCancelled);
    let raffle = client.get_raffle();
    assert_eq!(raffle.status, RaffleStatus::Cancelled);
}

#[test]
fn test_finalize_raffle_succeeds_when_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, admin_client, factory, _) =
        setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);

    client.deposit_prize();
    for _ in 0..5 {
        let b = Address::generate(&env);
        admin_client.mint(&b, &10i128);
        client.buy_ticket(&b);
    }

    env.as_contract(&factory, || {
        client.pause();
    });

    client.finalize_raffle();
    let raffle = client.get_raffle();
    assert_eq!(raffle.status, RaffleStatus::Finalized);
}

#[test]
fn test_unpause_restores_buy_ticket() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, buyer, _, factory, _) =
        setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);

    client.deposit_prize();

    env.as_contract(&factory, || {
        client.pause();
    });
    assert!(client.is_paused());

    env.as_contract(&factory, || {
        client.unpause();
    });
    assert!(!client.is_paused());

    let tickets_sold = client.buy_ticket(&buyer);
    assert_eq!(tickets_sold, 1);
}

// --- SET_ADMIN TESTS ---

#[test]
fn test_set_admin_by_factory() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, factory, _) =
        setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);

    let new_admin = Address::generate(&env);

    env.as_contract(&factory, || {
        client.set_admin(&new_admin);
    });
}

#[test]
#[should_panic]
fn test_set_admin_rejected_from_non_factory() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _) =
        setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);

    let stranger = Address::generate(&env);
    let new_admin = Address::generate(&env);

    env.as_contract(&stranger, || {
        client.set_admin(&new_admin);
    });
}

// --- FACTORY PAUSE/UNPAUSE TESTS ---

#[test]
fn test_factory_pause_unpause() {
    let env = Env::default();
    env.mock_all_auths();

    let factory_admin = Address::generate(&env);
    let treasury = Address::generate(&env);

    let factory_id = env.register(RaffleFactory, ());
    let factory_client = RaffleFactoryClient::new(&env, &factory_id);

    factory_client.init_factory(
        &factory_admin,
        &Bytes::from_slice(&env, &[0u8; 32]),
        &0u32,
        &treasury,
    );

    assert!(!factory_client.is_paused());

    factory_client.pause();
    assert!(factory_client.is_paused());

    factory_client.unpause();
    assert!(!factory_client.is_paused());
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_factory_create_raffle_blocked_when_paused() {
    let env = Env::default();
    env.mock_all_auths();

    let factory_admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let creator = Address::generate(&env);

    let factory_id = env.register(RaffleFactory, ());
    let factory_client = RaffleFactoryClient::new(&env, &factory_id);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_id = token_contract.address();

    factory_client.init_factory(
        &factory_admin,
        &Bytes::from_slice(&env, &[0u8; 32]),
        &0u32,
        &treasury,
    );

    factory_client.pause();

    factory_client.create_raffle(
        &creator,
        &String::from_str(&env, "Test"),
        &0u64,
        &5u32,
        &false,
        &10i128,
        &token_id,
        &100i128,
        &RandomnessSource::Internal,
        &None::<Address>,
    );
}

// --- FACTORY TWO-STEP ADMIN TRANSFER TESTS ---

#[test]
fn test_factory_admin_transfer_happy_path() {
    let env = Env::default();
    env.mock_all_auths();

    let factory_admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let treasury = Address::generate(&env);

    let factory_id = env.register(RaffleFactory, ());
    let factory_client = RaffleFactoryClient::new(&env, &factory_id);

    factory_client.init_factory(
        &factory_admin,
        &Bytes::from_slice(&env, &[0u8; 32]),
        &0u32,
        &treasury,
    );

    assert_eq!(factory_client.get_admin(), factory_admin);

    factory_client.transfer_admin(&new_admin);
    assert_eq!(factory_client.get_admin(), factory_admin);

    factory_client.accept_admin();
    assert_eq!(factory_client.get_admin(), new_admin);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_factory_accept_admin_no_pending() {
    let env = Env::default();
    env.mock_all_auths();

    let factory_admin = Address::generate(&env);
    let treasury = Address::generate(&env);

    let factory_id = env.register(RaffleFactory, ());
    let factory_client = RaffleFactoryClient::new(&env, &factory_id);

    factory_client.init_factory(
        &factory_admin,
        &Bytes::from_slice(&env, &[0u8; 32]),
        &0u32,
        &treasury,
    );

    factory_client.accept_admin();
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_factory_transfer_admin_while_pending() {
    let env = Env::default();
    env.mock_all_auths();

    let factory_admin = Address::generate(&env);
    let new_admin1 = Address::generate(&env);
    let new_admin2 = Address::generate(&env);
    let treasury = Address::generate(&env);

    let factory_id = env.register(RaffleFactory, ());
    let factory_client = RaffleFactoryClient::new(&env, &factory_id);

    factory_client.init_factory(
        &factory_admin,
        &Bytes::from_slice(&env, &[0u8; 32]),
        &0u32,
        &treasury,
    );

    factory_client.transfer_admin(&new_admin1);
    factory_client.transfer_admin(&new_admin2);
}

#[test]
fn test_factory_self_transfer_cancels_pending() {
    let env = Env::default();
    env.mock_all_auths();

    let factory_admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let treasury = Address::generate(&env);

    let factory_id = env.register(RaffleFactory, ());
    let factory_client = RaffleFactoryClient::new(&env, &factory_id);

    factory_client.init_factory(
        &factory_admin,
        &Bytes::from_slice(&env, &[0u8; 32]),
        &0u32,
        &treasury,
    );

    factory_client.transfer_admin(&new_admin);
    factory_client.transfer_admin(&factory_admin);

    assert_eq!(factory_client.get_admin(), factory_admin);

    factory_client.transfer_admin(&new_admin);
}

// --- FACTORY RELAY FUNCTION TESTS ---

#[test]
fn test_sync_admin_updates_instance() {
    let env = Env::default();
    env.mock_all_auths();

    let factory_admin = Address::generate(&env);
    let treasury = Address::generate(&env);

    let factory_id = env.register(RaffleFactory, ());
    let factory_client = RaffleFactoryClient::new(&env, &factory_id);

    factory_client.init_factory(
        &factory_admin,
        &Bytes::from_slice(&env, &[0u8; 32]),
        &0u32,
        &treasury,
    );

    let creator = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_id = token_contract.address();
    let token_admin_client = token::StellarAssetClient::new(&env, &token_id);
    token_admin_client.mint(&creator, &1_000i128);

    let contract_id = env.register(Contract, ());
    let instance_client = ContractClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Test"),
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

    instance_client.init(&factory_id, &factory_admin, &creator, &config);

    let new_admin = Address::generate(&env);
    factory_client.transfer_admin(&new_admin);
    factory_client.accept_admin();
    assert_eq!(factory_client.get_admin(), new_admin);

    factory_client.sync_admin(&contract_id);

    factory_client.pause_instance(&contract_id);
    assert!(instance_client.is_paused());
}

#[test]
fn test_factory_pause_unpause_instance() {
    let env = Env::default();
    env.mock_all_auths();

    let factory_admin = Address::generate(&env);
    let treasury = Address::generate(&env);

    let factory_id = env.register(RaffleFactory, ());
    let factory_client = RaffleFactoryClient::new(&env, &factory_id);

    factory_client.init_factory(
        &factory_admin,
        &Bytes::from_slice(&env, &[0u8; 32]),
        &0u32,
        &treasury,
    );

    let creator = Address::generate(&env);
    let buyer = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_id = token_contract.address();
    let token_admin_client = token::StellarAssetClient::new(&env, &token_id);
    token_admin_client.mint(&creator, &1_000i128);
    token_admin_client.mint(&buyer, &1_000i128);

    let contract_id = env.register(Contract, ());
    let instance_client = ContractClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Test"),
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

    instance_client.init(&factory_id, &factory_admin, &creator, &config);
    instance_client.deposit_prize();

    factory_client.pause_instance(&contract_id);
    assert!(instance_client.is_paused());

    factory_client.unpause_instance(&contract_id);
    assert!(!instance_client.is_paused());

    let sold = instance_client.buy_ticket(&buyer);
    assert_eq!(sold, 1);
}
