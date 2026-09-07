//! Prize claims, refunds, and authorization guards.

use super::*;

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

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

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
        metadata_hash: BytesN::from_array(&env, &[1u8; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        unique_winners: false,
            metadata_hash: BytesN::from_array(&env, &[1u8; 32]),
        claim_lockup_seconds: 0,
        swap_deadline_seconds: 0,
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
    assert!(raffle.winners.get(0).unwrap().address != attacker);

    env.ledger()
        .set_timestamp(2_000 + DEFAULT_CLAIM_LOCKUP_SECONDS + 1);

    let start_events = env.events().all().len();
    let result = client.try_claim_prize(&attacker, &0u32);
    assert_eq!(env.events().all().len(), start_events);
    assert_eq!(result, Err(Ok(Error::NotWinner)));
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
        unique_winners: false,
            metadata_hash: BytesN::from_array(&env, &[15; 32]),
        claim_lockup_seconds: 0,
    };

    client.init(&factory, &admin, &creator, &config);

    let start_events = env.events().all().len();
    let result = client.try_emergency_withdraw(&creator);
    assert_eq!(env.events().all().len(), start_events);
    assert_eq!(result.err(), Some(Ok(Error::PrizeNotDeposited)));
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
        unique_winners: false,
            metadata_hash: BytesN::from_array(&env, &[17; 32]),
        claim_lockup_seconds: 0,
        protocol_fee_bp: 1_000,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(&env, &[7; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();
    client.buy_tickets(&creator, &1);
    client.finalize_raffle();

    let before = client.get_raffle();
    assert_eq!(before.status, RaffleStatus::Finalized);
    assert!(before.prize_deposited);

    env.ledger()
        .set_timestamp(1_000 + EMERGENCY_WITHDRAW_DELAY_SECONDS + 1);

    client.emergency_withdraw(&creator);

    let after = client.get_raffle();
    assert_eq!(after.status, RaffleStatus::Cancelled);
    assert!(!after.prize_deposited);
    client.buy_tickets(&buyer, &1);
    client.finalize_raffle();

    env.ledger()
        .set_timestamp(1_000 + DEFAULT_CLAIM_LOCKUP_SECONDS + 1);
    let winner = client.get_raffle().winners.get(0).unwrap().address;
    let balance_before = soroban_sdk::token::Client::new(&env, &payment_token).balance(&winner);

    let claimed = client.claim_prize(&winner, &0);
    let gross = MIN_TICKET_PRICE * 10;
    
    let prize_fee = (gross * 1_000 + 9999) / 10_000;
    let net = gross - prize_fee;
    assert_eq!(claimed, gross); // The return value is the gross amount

    let balance_after = soroban_sdk::token::Client::new(&env, &payment_token).balance(&winner);
    assert_eq!(balance_after, balance_before + net);

    let ticket_fee = (MIN_TICKET_PRICE * 1_000 + 9999) / 10_000;
    assert_eq!(client.get_accumulated_fees(), ticket_fee + prize_fee);
}


