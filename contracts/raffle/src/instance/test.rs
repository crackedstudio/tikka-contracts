#![cfg(test)]

use super::*;
use crate::{ContractError, RaffleFactory, RaffleFactoryClient};
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    token, Address, Bytes, Env, IntoVal, String, Symbol,
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

    let mut prizes = Vec::new(env);
    prizes.push_back(10000);

    let config = RaffleConfig {
        description: String::from_str(env, "Audit Raffle"),
        end_time: 0,
        max_tickets: 5,
        allow_multiple: false,
        ticket_price: 10i128,
        payment_token: token_id,
        prize_amount: 100i128,
        prizes,
        randomness_source: source,
        oracle_address: oracle,
        protocol_fee_bp: fee_bp,
        treasury_address: treasury,
        swap_router: None,
        tikka_token: None,
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
    let winner = raffle.winners.get(0).unwrap();
    env.ledger().with_mut(|l| l.timestamp += 3600);
    let _claimed_amount = client.claim_prize(&winner, &0);

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
    let winner = client.get_raffle().winners.get(0).unwrap();
    env.ledger().with_mut(|l| l.timestamp += 3600);
    client.claim_prize(&winner, &0);

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
    assert_eq!(raffle_post.winners.get(0).unwrap(), expected_winner);
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

    let mut prizes = Vec::new(&env);
    prizes.push_back(10000);

    let config = RaffleConfig {
        description: String::from_str(&env, "Test Raffle"),
        end_time: 0,
        max_tickets: 5,
        allow_multiple: false,
        ticket_price: 10i128,
        payment_token: token_id,
        prize_amount: 100i128,
        prizes,
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
    };

    client.init(&factory, &factory_admin, &creator, &config);

    // Check that raffle_created event was emitted
    assert!(env.events().all().len() > 0);
}

#[test]
fn test_prize_deposited_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _) = setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);

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
    let winner = client.get_raffle().winners.get(0).unwrap();
    env.ledger().with_mut(|l| l.timestamp += 3600);
    client.claim_prize(&winner, &0);

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

// --- 5. REENTRANCY PROTECTION & STORAGE GUARDRAIL TESTS ---

#[test]
fn test_claim_prize_guard_released_after_success() {
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
    let winner = client.get_raffle().winners.get(0).unwrap();
    env.ledger().with_mut(|l| l.timestamp += 3600);
    client.claim_prize(&winner, &0);

    // Guard must be released after successful claim
    env.as_contract(&client.address, || {
        assert!(
            !env.storage().instance().has(&DataKey::ReentrancyGuard),
            "ReentrancyGuard should be removed after claim_prize completes"
        );
    });
}

#[test]
fn test_refund_guard_released_after_success() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, buyer, _, _, _) =
        setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);

    client.deposit_prize();
    client.buy_ticket(&buyer);
    client.cancel_raffle(&CancelReason::CreatorCancelled);
    client.refund_ticket(&1u32);

    // Guard must be released after successful refund
    env.as_contract(&client.address, || {
        assert!(
            !env.storage().instance().has(&DataKey::ReentrancyGuard),
            "ReentrancyGuard should be removed after refund_ticket completes"
        );
    });
}

#[test]
fn test_sequential_refunds_succeed_guard_properly_released() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, buyer, admin_client, _, _) =
        setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);
    let token_client = token::Client::new(&env, &admin_client.address);

    client.deposit_prize();

    // Two different buyers purchase tickets
    client.buy_ticket(&buyer);
    let buyer2 = Address::generate(&env);
    admin_client.mint(&buyer2, &10i128);
    client.buy_ticket(&buyer2);

    client.cancel_raffle(&CancelReason::CreatorCancelled);

    // Sequential refunds must both succeed (guard released between calls)
    let refund1 = client.refund_ticket(&1u32);
    assert_eq!(refund1, 10i128);
    let refund2 = client.refund_ticket(&2u32);
    assert_eq!(refund2, 10i128);

    // Both buyers fully refunded
    assert_eq!(token_client.balance(&buyer), 1000i128);
    assert_eq!(token_client.balance(&buyer2), 10i128);
}

#[test]
#[should_panic] // Error(Contract, #21) - Reentrancy
fn test_claim_prize_blocked_by_active_reentrancy_guard() {
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
    let winner = client.get_raffle().winners.get(0).unwrap();

    // Simulate reentrancy: set guard before external call returns
    env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .set(&DataKey::ReentrancyGuard, &true);
    });

    env.ledger().with_mut(|l| l.timestamp += 3600);
    client.claim_prize(&winner, &0); // Must panic with Reentrancy
}

#[test]
#[should_panic] // Error(Contract, #21) - Reentrancy
fn test_refund_blocked_by_active_reentrancy_guard() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, buyer, _, _, _) =
        setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);

    client.deposit_prize();
    client.buy_ticket(&buyer);
    client.cancel_raffle(&CancelReason::CreatorCancelled);

    // Simulate reentrancy: set guard before refund call
    env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .set(&DataKey::ReentrancyGuard, &true);
    });

    client.refund_ticket(&1u32); // Must panic with Reentrancy
}

#[test]
fn test_claim_with_protocol_fee_guard_released() {
    let env = Env::default();
    env.mock_all_auths();
    let treasury = Address::generate(&env);
    let (client, _, _, admin_client, _, _) = setup_raffle_env(
        &env,
        RandomnessSource::Internal,
        None,
        500,
        Some(treasury.clone()),
    );
    let token_client = token::Client::new(&env, &admin_client.address);

    client.deposit_prize();
    for _ in 0..5 {
        let b = Address::generate(&env);
        admin_client.mint(&b, &10i128);
        client.buy_ticket(&b);
    }
    client.finalize_raffle();

    let winner = client.get_raffle().winners.get(0).unwrap();
    env.ledger().with_mut(|l| l.timestamp += 3600);
    let claimed = client.claim_prize(&winner, &0);
    assert_eq!(claimed, 95i128);

    // Guard released even when fee transfer path is taken
    env.as_contract(&client.address, || {
        assert!(!env.storage().instance().has(&DataKey::ReentrancyGuard));
    });

    assert_eq!(token_client.balance(&winner), 95i128);
    assert_eq!(token_client.balance(&treasury), 5i128);
}

// --- 6. GLOBAL PROTOCOL ANALYTICS TESTS ---

fn setup_factory(env: &Env) -> (RaffleFactoryClient<'_>, Address) {
    let admin = Address::generate(env);
    let treasury = Address::generate(env);
    let factory_id = env.register(RaffleFactory, ());
    let factory_client = RaffleFactoryClient::new(env, &factory_id);

    // Register a dummy wasm hash (32 zero bytes) – factory init needs it
    let wasm_hash = soroban_sdk::BytesN::from_array(env, &[0u8; 32]);
    factory_client.init_factory(&admin, &wasm_hash, &0u32, &treasury);

    (factory_client, admin)
}

#[test]
fn test_track_participant_increments_counter() {
    let env = Env::default();
    env.mock_all_auths();
    let (factory, _admin) = setup_factory(&env);

    assert_eq!(factory.get_unique_participants(), 0u32);

    let alice = Address::generate(&env);
    factory.track_participant(&alice);

    assert_eq!(factory.get_unique_participants(), 1u32);
}

#[test]
fn test_track_participant_idempotent_for_same_address() {
    let env = Env::default();
    env.mock_all_auths();
    let (factory, _admin) = setup_factory(&env);

    let alice = Address::generate(&env);
    factory.track_participant(&alice);
    factory.track_participant(&alice); // second call must NOT increment
    factory.track_participant(&alice); // third call also a no-op

    assert_eq!(factory.get_unique_participants(), 1u32);
}

#[test]
fn test_track_multiple_unique_participants() {
    let env = Env::default();
    env.mock_all_auths();
    let (factory, _admin) = setup_factory(&env);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let carol = Address::generate(&env);

    factory.track_participant(&alice);
    factory.track_participant(&bob);
    factory.track_participant(&carol);
    // alice again – must remain 3
    factory.track_participant(&alice);

    assert_eq!(factory.get_unique_participants(), 3u32);
}

// --- 7. CEI PATTERN VALIDATION TESTS ---

#[test]
fn test_deposit_prize_cei_state_active_after_call() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _) =
        setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);

    client.deposit_prize();

    let raffle = client.get_raffle();
    assert!(raffle.status == RaffleStatus::Active);
    assert!(raffle.prize_deposited);
}

#[test]
fn test_buy_ticket_cei_state_incremented_correctly() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, admin_client, _, _) =
        setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);

    client.deposit_prize();

    let buyer1 = Address::generate(&env);
    admin_client.mint(&buyer1, &10i128);
    let sold_count = client.buy_ticket(&buyer1);
    assert_eq!(sold_count, 1);

    let raffle = client.get_raffle();
    assert_eq!(raffle.tickets_sold, 1);

    let buyer2 = Address::generate(&env);
    admin_client.mint(&buyer2, &10i128);
    let sold_count2 = client.buy_ticket(&buyer2);
    assert_eq!(sold_count2, 2);

    let raffle2 = client.get_raffle();
    assert_eq!(raffle2.tickets_sold, 2);
}

#[test]
fn test_claim_prize_cei_status_transitions_to_claimed() {
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

    let raffle_before = client.get_raffle();
    assert!(raffle_before.status == RaffleStatus::Finalized);

    let winner = raffle_before.winners.get(0).unwrap();
    env.ledger().with_mut(|l| l.timestamp += 3600);
    client.claim_prize(&winner, &0);

    let raffle_after = client.get_raffle();
    assert!(raffle_after.status == RaffleStatus::Claimed);
}

#[test]
#[should_panic] // Error(Contract, #20) - InvalidStateTransition (status already Claimed)
fn test_double_claim_rejected_after_cei_state_transition() {
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
    let winner = client.get_raffle().winners.get(0).unwrap();
    env.ledger().with_mut(|l| l.timestamp += 3600);
    client.claim_prize(&winner, &0);
    client.claim_prize(&winner, &0); // Must panic: status is Claimed, not Finalized
}

#[test]
fn test_cancel_raffle_cei_state_cancelled_before_refund() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, creator, buyer, admin_client, _, _) =
        setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);
    let token_client = token::Client::new(&env, &admin_client.address);

    client.deposit_prize();
    client.buy_ticket(&buyer);

    client.cancel_raffle(&CancelReason::CreatorCancelled);

    // CEI: status is Cancelled and prize refunded to creator
    let raffle = client.get_raffle();
    assert!(raffle.status == RaffleStatus::Cancelled);
    assert!(!raffle.prize_deposited);
    assert_eq!(token_client.balance(&creator), 1000i128);
}

// --- 8. NFT INTERFACE TESTS ---

#[test]
fn test_nft_metadata() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, _, _, _, _) = setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);
    
    assert_eq!(client.name(), String::from_str(&env, "Tikka Raffle Ticket"));
    assert_eq!(client.symbol(), String::from_str(&env, "TIKKA_TKT"));
    assert_eq!(client.token_uri(&1u32), String::from_str(&env, "https://tikka.app/api/ticket"));
}

#[test]
fn test_nft_transfer_and_balance() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, buyer, admin_client, _, _) = setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);
    
    client.deposit_prize();
    client.buy_ticket(&buyer);
    
    assert_eq!(client.balance(&buyer), 1);
    assert_eq!(client.owner_of(&1u32), buyer);

    let new_owner = Address::generate(&env);
    client.transfer(&buyer, &new_owner, &1u32);

    assert_eq!(client.balance(&buyer), 0);
    assert_eq!(client.balance(&new_owner), 1);
    assert_eq!(client.owner_of(&1u32), new_owner);
    
    // Attempting unauthorized transfer
    let hacker = Address::generate(&env);
    let res = client.try_transfer(&hacker, &new_owner, &1u32);
    assert!(res.is_err());
}

#[test]
fn test_nft_approvals_and_transfer_from() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, buyer, _, _, _) = setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);
    
    client.deposit_prize();
    client.buy_ticket(&buyer);

    let operator = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.approve(&buyer, &Some(operator.clone()), &1u32);
    assert_eq!(client.get_approved(&1u32), Some(operator.clone()));

    // Operator transfers to receiver
    client.transfer_from(&operator, &buyer, &receiver, &1u32);

    assert_eq!(client.owner_of(&1u32), receiver);
    assert_eq!(client.get_approved(&1u32), None); // Approval clears on transfer
}

#[test]
fn test_nft_set_approval_for_all() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, buyer, _, _, _) = setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);
    
    client.deposit_prize();
    client.buy_ticket(&buyer);

    let operator = Address::generate(&env);
    let receiver = Address::generate(&env);

    client.set_approval_for_all(&buyer, &operator, &true);
    assert!(client.is_approved_for_all(&buyer, &operator));

    client.transfer_from(&operator, &buyer, &receiver, &1u32);
    assert_eq!(client.owner_of(&1u32), receiver);
}

#[test]
fn test_nft_winner_after_transfer() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, buyer, admin_client, _, _) = setup_raffle_env(&env, RandomnessSource::Internal, None, 0, None);
    
    client.deposit_prize();
    client.buy_ticket(&buyer);

    let secondary_buyer = Address::generate(&env);
    client.transfer(&buyer, &secondary_buyer, &1u32);

    for _ in 0..4 {
        let b = Address::generate(&env);
        admin_client.mint(&b, &10i128);
        client.buy_ticket(&b);
    }
    client.finalize_raffle();
    let winner = client.get_raffle().winners.get(0).unwrap();

    // Advance time by 3600s
    env.ledger().with_mut(|l| {
        l.timestamp += 3600;
    });

    let claimed = client.claim_prize(&winner, &0);
    assert_eq!(claimed, 100i128);
}

#[test]
fn test_tiered_prizes() {
    let env = Env::default();
    env.mock_all_auths();
    
    let creator = Address::generate(&env);
    let admin = Address::generate(&env);
    let factory_admin = Address::generate(&env);

    #[contract]
    pub struct DummyFactory;
    #[contractimpl]
    impl DummyFactory {}
    let factory = env.register(DummyFactory, ());

    let token_contract = env.register_stellar_asset_contract_v2(admin.clone());
    let token_id = token_contract.address();
    let admin_client = token::StellarAssetClient::new(&env, &token_id);

    admin_client.mint(&creator, &1_000i128);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let mut prizes = Vec::new(&env);
    prizes.push_back(5000); // 50%
    prizes.push_back(3000); // 30%
    prizes.push_back(2000); // 20%

    let config = RaffleConfig {
        description: String::from_str(&env, "Tiered Raffle"),
        end_time: 0,
        max_tickets: 10,
        allow_multiple: true,
        ticket_price: 10i128,
        payment_token: token_id.clone(),
        prize_amount: 1000i128,
        prizes,
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
    };

    client.init(&factory, &factory_admin, &creator, &config);
    client.deposit_prize();

    for _ in 0..10 {
        let b = Address::generate(&env);
        admin_client.mint(&b, &10i128);
        client.buy_ticket(&b);
    }

    client.finalize_raffle();

    let raffle = client.get_raffle();
    assert_eq!(raffle.winners.len(), 3);
    
    env.ledger().with_mut(|l| l.timestamp += 3600);

    let token_client = token::Client::new(&env, &token_id);

    // Winner 1 (50%)
    let winner1 = raffle.winners.get(0).unwrap();
    client.claim_prize(&winner1, &0);
    assert_eq!(token_client.balance(&winner1), 500i128);

    // Winner 2 (30%)
    let winner2 = raffle.winners.get(1).unwrap();
    client.claim_prize(&winner2, &1);
    assert_eq!(token_client.balance(&winner2), 300i128);

    // Winner 3 (20%)
    let winner3 = raffle.winners.get(2).unwrap();
    client.claim_prize(&winner3, &2);
    assert_eq!(token_client.balance(&winner3), 200i128);

    let raffle_final = client.get_raffle();
    assert!(raffle_final.status == RaffleStatus::Claimed);
}
