#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::StellarAssetClient,
    Address, BytesN, Env, String,
};
use raffle_shared::{DEFAULT_CLAIM_LOCKUP_SECONDS, DEFAULT_SWAP_DEADLINE_SECONDS};

#[test]
fn test_oracle_fallback_with_ledger_delays() {
    let env = Env::default();
    env.mock_all_auths();

    // 1. Setup factory, admin, creator
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

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    // 2. Initialize Raffle with External Randomness
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
        metadata_hash: BytesN::from_array(&env, &[1; 32]),
        claim_lockup_seconds: 0,
        swap_deadline_seconds: 0,
    };

    client.init(&factory, &admin, &creator, &config);

    // Verify that defaults were resolved (0 values replaced with defaults)
    let raffle = client.get_raffle();
    assert_eq!(raffle.claim_lockup_seconds, DEFAULT_CLAIM_LOCKUP_SECONDS);
    assert_eq!(raffle.swap_deadline_seconds, DEFAULT_SWAP_DEADLINE_SECONDS);

    // Remove factory from storage so buy_tickets skips the factory code path
    env.as_contract(&contract_id, || {
        env.storage().instance().remove(&DataKey::Factory);
    });

    // 3. Deposit prize and buy ticket
    client.deposit_prize();
    client.buy_tickets(&creator, &10);

    // 4. Finalize raffle (requests randomness)
    client.finalize_raffle();

    // 5. Ensure it's in Drawing state and requested randomness
    let raffle = client.get_raffle();
    assert_eq!(raffle.status, RaffleStatus::Drawing);

    // 6. Attempt fallback too early
    let result = client.try_trigger_randomness_fallback(&creator, &false);
    assert_eq!(result.err(), Some(Ok(Error::FallbackTooEarly)));

    // 7. Simulate ledger delays
    env.ledger().with_mut(|l| {
        l.sequence_number += ORACLE_TIMEOUT_LEDGERS + 1;
        l.timestamp += 86400; // 1 day
    });

    // 8. Trigger fallback successfully (no refund — finalize)
    client.trigger_randomness_fallback(&creator, &false);

    // 9. Verify finalized state
    let raffle_after = client.get_raffle();
    assert_eq!(raffle_after.status, RaffleStatus::Finalized);

    // We can also verify the fairness data
    let fairness = client.get_fairness_data();
    assert_eq!(fairness.randomness_source, RandomnessSource::External);
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

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

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
        protocol_fee_bp: 100,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(&env, &[2; 32]),
        claim_lockup_seconds: 0,
        swap_deadline_seconds: 0,
    };

    client.init(&factory, &admin, &creator, &config);

    // Verify that defaults were resolved
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

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

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
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(&env, &[3; 32]),
        claim_lockup_seconds: 0,
        swap_deadline_seconds: 0,
    };

    client.init(&factory, &admin, &creator, &config);

    // Verify that defaults were resolved
    let raffle = client.get_raffle();
    assert_eq!(raffle.claim_lockup_seconds, DEFAULT_CLAIM_LOCKUP_SECONDS);
    assert_eq!(raffle.swap_deadline_seconds, DEFAULT_SWAP_DEADLINE_SECONDS);

    client.set_protocol_fee_bp(&500);

    let raffle = client.get_raffle();
    assert_eq!(raffle.protocol_fee_bp, 500);
}

#[test]
fn test_admin_withdraws_accumulated_fees() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let buyer = Address::generate(&env);
    let recipient = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = StellarAssetClient::new(&env, &payment_token);
    token_client.mint(&creator, &1_000_000);
    token_client.mint(&buyer, &1_000_000);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Fee withdraw"),
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
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(&env, &[4; 32]),
        claim_lockup_seconds: 0,
        swap_deadline_seconds: 0,
    };

    client.init(&factory, &admin, &creator, &config);

    // Verify that defaults were resolved
    let raffle = client.get_raffle();
    assert_eq!(raffle.claim_lockup_seconds, DEFAULT_CLAIM_LOCKUP_SECONDS);
    assert_eq!(raffle.swap_deadline_seconds, DEFAULT_SWAP_DEADLINE_SECONDS);

    client.deposit_prize();
    client.buy_tickets(&buyer, &1);
    client.finalize_raffle();

    env.ledger().set_timestamp(1_000 + DEFAULT_CLAIM_LOCKUP_SECONDS + 1);
    let winner = client.get_raffle().winners.get(0).unwrap();
    client.claim_prize(&winner, &0);

    let fee_amount = MIN_TICKET_PRICE * 1_000 / 10_000;
    assert_eq!(client.get_accumulated_fees(), fee_amount);

    client.withdraw_fees(&recipient, &fee_amount);
    assert_eq!(client.get_accumulated_fees(), 0);
    assert_eq!(
        soroban_sdk::token::Client::new(&env, &payment_token).balance(&recipient),
        fee_amount
    );
}

#[test]
fn test_buy_tickets_rejects_quantity_above_per_tx_cap() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let buyer = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = StellarAssetClient::new(&env, &payment_token);
    token_client.mint(&creator, &1_000_000);
    token_client.mint(&buyer, &1_000_000);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Per-tx cap"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 100,
        max_tickets_per_tx: 5,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: payment_token.clone(),
        prize_amount: MIN_TICKET_PRICE * 100,
        prizes: soroban_sdk::vec![&env, 10000],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(&env, &[5; 32]),
        claim_lockup_seconds: 0,
        swap_deadline_seconds: 0,
    };

    client.init(&factory, &admin, &creator, &config);

    // Verify that defaults were resolved
    let raffle = client.get_raffle();
    assert_eq!(raffle.claim_lockup_seconds, DEFAULT_CLAIM_LOCKUP_SECONDS);
    assert_eq!(raffle.swap_deadline_seconds, DEFAULT_SWAP_DEADLINE_SECONDS);

    env.as_contract(&contract_id, || {
        env.storage().instance().remove(&DataKey::Factory);
    });
    client.deposit_prize();

    assert_eq!(
        client.try_buy_tickets(&buyer, &6),
        Err(Ok(Error::ExceedsMaxTicketsPerTx))
    );
    assert_eq!(client.buy_tickets(&buyer, &5), 5);
}

#[test]
fn test_finalize_raffle_sets_drawing_lock_and_blocks_reentry() {
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

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Drawing lock test"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 1,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: payment_token.clone(),
        prize_amount: MIN_TICKET_PRICE,
        prizes: soroban_sdk::vec![&env, 10000],
        randomness_source: RandomnessSource::External,
        oracle_address: Some(Address::generate(&env)),
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(&env, &[7; 32]),
        claim_lockup_seconds: 0,
        swap_deadline_seconds: 0,
    };

    client.init(&factory, &admin, &creator, &config);

    // Verify that defaults were resolved
    let raffle = client.get_raffle();
    assert_eq!(raffle.claim_lockup_seconds, DEFAULT_CLAIM_LOCKUP_SECONDS);
    assert_eq!(raffle.swap_deadline_seconds, DEFAULT_SWAP_DEADLINE_SECONDS);

    client.deposit_prize();
    client.buy_tickets(&creator, &1);

    client.finalize_raffle();

    let drawing_lock: bool = env
        .as_contract(&contract_id, || {
            env.storage()
                .instance()
                .get(&crate::DataKey::DrawingLock)
                .unwrap_or(false)
        });
    let raffle = client.get_raffle();
    assert!(drawing_lock);
    assert_eq!(raffle.status, RaffleStatus::Drawing);

    let second_result = client.try_finalize_raffle();
    assert_eq!(second_result.err(), Some(Ok(crate::Error::DrawingAlreadyInProgress)));

    let raffle_after = client.get_raffle();
    assert_eq!(raffle_after.status, RaffleStatus::Drawing);
    let drawing_lock_after: bool = env
        .as_contract(&contract_id, || {
            env.storage()
                .instance()
                .get(&crate::DataKey::DrawingLock)
                .unwrap_or(false)
        });
    assert!(drawing_lock_after);
}

#[test]
fn test_finalize_rollback_on_randomness_request_failure() {
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

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Rollback test"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 1,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: payment_token.clone(),
        prize_amount: MIN_TICKET_PRICE,
        prizes: soroban_sdk::vec![&env, 10000],
        randomness_source: RandomnessSource::External,
        oracle_address: Some(Address::generate(&env)),
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(&env, &[8; 32]),
        claim_lockup_seconds: 0,
        swap_deadline_seconds: 0,
    };

    client.init(&factory, &admin, &creator, &config);

    // Verify that defaults were resolved
    let raffle = client.get_raffle();
    assert_eq!(raffle.claim_lockup_seconds, DEFAULT_CLAIM_LOCKUP_SECONDS);
    assert_eq!(raffle.swap_deadline_seconds, DEFAULT_SWAP_DEADLINE_SECONDS);

    client.deposit_prize();
    client.buy_tickets(&creator, &1);

    env.as_contract(&contract_id, || {
        env.storage().instance().set(&crate::DataKey::RandomnessRequested, &true);
    });

    let result = client.try_finalize_raffle();
    assert_eq!(result.err(), Some(Ok(crate::Error::RandomnessAlreadyRequested)));

    let raffle = client.get_raffle();
    assert_eq!(raffle.status, RaffleStatus::Active);
    let drawing_lock: bool = env
        .as_contract(&contract_id, || {
            env.storage()
                .instance()
                .get(&crate::DataKey::DrawingLock)
                .unwrap_or(false)
        });
    assert!(!drawing_lock);
}

#[test]
fn test_allow_multiple_false_single_ticket_per_buyer() {
    let env = Env::default();
    env.mock_all_auths();

    // ARRANGE
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

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Test allow_multiple=false"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 10,
        min_tickets: 1,
        allow_multiple: false,
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
        metadata_hash: BytesN::from_array(&env, &[6; 32]),
        claim_lockup_seconds: 0,
        swap_deadline_seconds: 0,
    };

    client.init(&factory, &admin, &creator, &config);

    // Verify that defaults were resolved
    let raffle = client.get_raffle();
    assert_eq!(raffle.claim_lockup_seconds, DEFAULT_CLAIM_LOCKUP_SECONDS);
    assert_eq!(raffle.swap_deadline_seconds, DEFAULT_SWAP_DEADLINE_SECONDS);

    client.deposit_prize();

    // ACT: Buyer A buys first ticket
    client.buy_tickets(&buyer_a, &1);

    // ASSERT: Buyer A has 1 ticket, tickets_sold = 1
    let raffle = client.get_raffle();
    assert_eq!(raffle.tickets_sold, 1);
    let buyer_a_count: u32 = env
        .as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get(&crate::DataKey::TicketCount(buyer_a.clone()))
                .unwrap_or(0)
        });
    assert_eq!(buyer_a_count, 1);

    // ACT: Buyer A tries to buy another ticket (should fail)
    let result = client.try_buy_tickets(&buyer_a, &1);
    assert_eq!(
        result.err(),
        Some(Ok(crate::Error::MultipleTicketsNotAllowed))
    );

    // ASSERT: State unchanged
    let raffle_after = client.get_raffle();
    assert_eq!(raffle_after.tickets_sold, 1);
    let buyer_a_count_after: u32 = env
        .as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get(&crate::DataKey::TicketCount(buyer_a.clone()))
                .unwrap_or(0)
        });
    assert_eq!(buyer_a_count_after, 1);

    // ACT: Buyer B buys a ticket (should succeed)
    client.buy_tickets(&buyer_b, &1);

    // ASSERT: Buyer B has 1 ticket, tickets_sold = 2
    let raffle_final = client.get_raffle();
    assert_eq!(raffle_final.tickets_sold, 2);
    let buyer_b_count: u32 = env
        .as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get(&crate::DataKey::TicketCount(buyer_b.clone()))
                .unwrap_or(0)
        });
    assert_eq!(buyer_b_count, 1);
}

#[test]
fn emergency_withdraw_fails_before_delay_in_finalized_state() {
fn test_refund_ticket_after_cancel() {
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

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

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
        metadata_hash: BytesN::from_array(&env, &[9; 32]),
        claim_lockup_seconds: 0,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();
    client.buy_tickets(&creator, &1);
    client.finalize_raffle();

    let result = client.try_emergency_withdraw(&creator);
    assert_eq!(result.err(), Some(Ok(Error::EmergencyTooEarly)));
}

#[test]
fn emergency_withdraw_succeeds_after_delay_in_finalized_state() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let buyer = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = StellarAssetClient::new(&env, &payment_token);
    token_client.mint(&creator, &1_000_000);
    token_client.mint(&buyer, &1_000_000);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

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
        metadata_hash: BytesN::from_array(&env, &[10; 32]),
        claim_lockup_seconds: 0,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();
    client.buy_tickets(&creator, &1);
    client.finalize_raffle();

    env.ledger().set_timestamp(1_000 + EMERGENCY_WITHDRAW_DELAY_SECONDS + 1);

    client.emergency_withdraw(&creator);
    let raffle = client.get_raffle();
    assert_eq!(raffle.status, RaffleStatus::Cancelled);
    assert!(!raffle.prize_deposited);
}

#[test]
fn emergency_withdraw_fails_for_no_deadline_raffle_before_timeout() {
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

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Test"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 1,
        max_tickets_per_tx: 1,
        description: String::from_str(&env, "Refund test"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 5,
        max_tickets_per_tx: 5,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: payment_token.clone(),
        prize_amount: MIN_TICKET_PRICE * 10,
        prize_amount: MIN_TICKET_PRICE * 5,
        prizes: soroban_sdk::vec![&env, 10000],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(&env, &[11; 32]),
        claim_lockup_seconds: 0,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();
    client.buy_tickets(&creator, &1);
    client.finalize_raffle();

    let result = client.try_emergency_withdraw(&creator);
    assert_eq!(result.err(), Some(Ok(Error::EmergencyTooEarly)));
}

#[test]
fn emergency_withdraw_succeeds_for_no_deadline_drawing_raffle_after_timeout() {
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

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

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
        metadata_hash: BytesN::from_array(&env, &[12; 32]),
        claim_lockup_seconds: 0,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();
    client.buy_tickets(&creator, &1);
    client.finalize_raffle();

    env.ledger().set_timestamp(2_000 + EMERGENCY_WITHDRAW_DELAY_SECONDS + 1);

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

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

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
        metadata_hash: BytesN::from_array(&env, &[13; 32]),
        claim_lockup_seconds: 0,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();

    let result = client.try_emergency_withdraw(&creator);
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

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

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
        metadata_hash: BytesN::from_array(&env, &[14; 32]),
        claim_lockup_seconds: 0,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();
    client.cancel(&creator, &CancelReason::Other);

    let result = client.try_emergency_withdraw(&creator);
    assert_eq!(result.err(), Some(Ok(Error::InvalidStatus)));
}

#[test]
fn emergency_withdraw_fails_if_prize_not_deposited() {
        metadata_hash: BytesN::from_array(&env, &[5; 32]),
        claim_lockup_seconds: 0,
        swap_deadline_seconds: 0,
    };

    client.init(&factory, &admin, &creator, &config);
    env.as_contract(&contract_id, || {
        env.storage().instance().remove(&DataKey::Factory);
    });

    client.deposit_prize();
    client.buy_tickets(&buyer, &1);

    let balance_before = soroban_sdk::token::Client::new(&env, &payment_token).balance(&buyer);
    client.cancel_raffle(&CancelReason::CreatorCancelled);

    let refunded = client.refund_ticket(&1);
    assert_eq!(refunded, MIN_TICKET_PRICE);

    let balance_after = soroban_sdk::token::Client::new(&env, &payment_token).balance(&buyer);
    assert_eq!(balance_after, balance_before + MIN_TICKET_PRICE);

    let second_refund = client.try_refund_ticket(&1);
    assert_eq!(second_refund.err(), Some(Ok(Error::PrizeAlreadyClaimed)));
}

#[test]
fn test_refund_guard_released_after_success() {
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

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

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
        metadata_hash: BytesN::from_array(&env, &[15; 32]),
        claim_lockup_seconds: 0,
    };

    client.init(&factory, &admin, &creator, &config);

    let result = client.try_emergency_withdraw(&creator);
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
    let buyer = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = StellarAssetClient::new(&env, &payment_token);
    token_client.mint(&creator, &1_000_000);
    token_client.mint(&buyer, &1_000_000);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Test"),
        end_time: 2_000,
        no_deadline: false,
        max_tickets: 1,
        max_tickets_per_tx: 1,
        description: String::from_str(&env, "Guard release"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 5,
        max_tickets_per_tx: 5,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: payment_token.clone(),
        prize_amount: MIN_TICKET_PRICE * 10,
        prize_amount: MIN_TICKET_PRICE * 5,
        prizes: soroban_sdk::vec![&env, 10000],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(&env, &[16; 32]),
        claim_lockup_seconds: 0,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();
    client.buy_tickets(&creator, &1);
    client.finalize_raffle();

    env.ledger().set_timestamp(1_000 + EMERGENCY_WITHDRAW_DELAY_SECONDS + 1);

    let stranger_result = client.try_emergency_withdraw(&stranger);
    assert_eq!(stranger_result.err(), Some(Ok(Error::NotAuthorized)));

    client.emergency_withdraw(&admin);
}

#[test]
fn emergency_withdraw_sets_status_to_cancelled_and_clears_prize_deposited() {
        metadata_hash: BytesN::from_array(&env, &[6; 32]),
        claim_lockup_seconds: 0,
        swap_deadline_seconds: 0,
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
fn test_claim_prize_pays_full_gross_with_protocol_fee() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let buyer = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = StellarAssetClient::new(&env, &payment_token);
    token_client.mint(&creator, &1_000_000);
    token_client.mint(&buyer, &1_000_000);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Test"),
        end_time: 2_000,
        no_deadline: false,
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
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(&env, &[17; 32]),
        claim_lockup_seconds: 0,
        protocol_fee_bp: 1_000,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(&env, &[7; 32]),
        claim_lockup_seconds: 0,
        swap_deadline_seconds: 0,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();
    client.buy_tickets(&creator, &1);
    client.finalize_raffle();

    let before = client.get_raffle();
    assert_eq!(before.status, RaffleStatus::Finalized);
    assert!(before.prize_deposited);

    env.ledger().set_timestamp(1_000 + EMERGENCY_WITHDRAW_DELAY_SECONDS + 1);

    client.emergency_withdraw(&creator);

    let after = client.get_raffle();
    assert_eq!(after.status, RaffleStatus::Cancelled);
    assert!(!after.prize_deposited);
    client.buy_tickets(&buyer, &1);
    client.finalize_raffle();

    env.ledger().set_timestamp(1_000 + DEFAULT_CLAIM_LOCKUP_SECONDS + 1);
    let winner = client.get_raffle().winners.get(0).unwrap();
    let balance_before = soroban_sdk::token::Client::new(&env, &payment_token).balance(&winner);

    let claimed = client.claim_prize(&winner, &0);
    let gross = MIN_TICKET_PRICE * 10;
    assert_eq!(claimed, gross);

    let balance_after = soroban_sdk::token::Client::new(&env, &payment_token).balance(&winner);
    assert_eq!(balance_after, balance_before + gross);

    let ticket_fee = MIN_TICKET_PRICE * 1_000 / 10_000;
    assert_eq!(client.get_accumulated_fees(), ticket_fee);
}
