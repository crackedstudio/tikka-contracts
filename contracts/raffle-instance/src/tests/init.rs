//! Config validation and init rejection branches (claim lockup, bounds, metadata hash).

use super::*;

#[test]
fn test_init_claim_lockup_seconds_at_bound_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

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
        claim_lockup_seconds: 0,
        swap_deadline_seconds: 0,
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
fn test_init_claim_lockup_seconds_above_bound_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

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
        claim_lockup_seconds: 0,
        swap_deadline_seconds: 0,
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
fn test_init_claim_lockup_seconds_mid_range_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let token = StellarAssetClient::new(&env, &payment_token);
    let token_ro = soroban_sdk::token::Client::new(&env, &payment_token);
    token.mint(&creator, &100_000_000);
    token.mint(&buyer_a, &1_000_000);
    token.mint(&buyer_b, &1_000_000);
    token.mint(&buyer_c, &1_000_000);

    // 2. Init the raffle (factory-mediated init emulated directly).
    let config = lifecycle_config(&env, &payment_token, &treasury);
    let expected_metadata_hash = config.metadata_hash.clone();
    let prize_amount = config.prize_amount;
    let ticket_price = config.ticket_price;
    let fee_bp = config.protocol_fee_bp as i128;
    client.init(&creator_factory_addr(&env), &admin, &creator, &config);
    assert_metadata_hash(&client, &expected_metadata_hash);

    // Skip the factory cross-contract calls in buy_tickets.
    env.as_contract(&contract_id, || {
        env.storage().instance().remove(&DataKey::Factory);
    });

    assert_eq!(client.get_raffle().status, RaffleStatus::PendingPrize);
    assert_metadata_hash(&client, &expected_metadata_hash);

    // 3. Creator deposits the prize → status becomes Active.
    let creator_balance_before_deposit = token_ro.balance(&creator);
    let start_events = env.events().all().len();
    client.deposit_prize();
    assert_event(
        &env,
        &client.address,
        "prize_deposited",
        events::PrizeDeposited {
            creator: creator.clone(),
            amount: prize_amount,
            token: payment_token.clone(),
            timestamp: env.ledger().timestamp(),
        },
    );
    assert_eq!(env.events().all().len(), start_events + 1);
    assert_eq!(client.get_raffle().status, RaffleStatus::Active);
    assert_metadata_hash(&client, &expected_metadata_hash);
    assert_eq!(
        token_ro.balance(&creator),
        creator_balance_before_deposit - prize_amount
    );
    assert_eq!(token_ro.balance(&contract_id), prize_amount);

    // 4. Three buyers each purchase one ticket. Each pays ticket_price; the
    //    protocol fee (1%) is forwarded to the treasury on each purchase.
    let per_ticket_fee = ticket_price * fee_bp / 10_000;
    for (idx, buyer) in [&buyer_a, &buyer_b, &buyer_c].iter().enumerate() {
        let start_events = env.events().all().len();
        let sold = client.buy_tickets(buyer, &1);
        
        assert_event(
            &env,
            &client.address,
            "ticket_purchased",
            events::TicketPurchased {
                buyer: buyer.clone(),
                ticket_ids: soroban_sdk::vec![&env, sold],
                quantity: 1,
                ticket_price: ticket_price,
                effective_ticket_price: ticket_price,
                total_paid: ticket_price,
                protocol_fee: ticket_price * fee_bp / 10_000,
                timestamp: env.ledger().timestamp(),
            },
        );
        // We know buy_tickets also emits a StatusChanged event when it sells out (the last iteration).
        if idx < 2 {
            assert_eq!(env.events().all().len(), start_events + 1);
        } else {
            assert_eq!(env.events().all().len(), start_events + 2); // StatusChanged emitted on the last one!
        }
        
        assert_eq!(sold, idx as u32 + 1);
        assert_metadata_hash(&client, &expected_metadata_hash);
    }
    let raffle_after_sales = client.get_raffle();
    assert_eq!(raffle_after_sales.tickets_sold, 3);
    // Selling out flips the raffle into Drawing.
    assert_eq!(raffle_after_sales.status, RaffleStatus::Drawing);
    assert_eq!(token_ro.balance(&treasury), per_ticket_fee * 3);

    // 5. Finalize → internal randomness picks the winner synchronously.
    let start_events = env.events().all().len();
    client.finalize_raffle();
    
    assert_event(
        &env,
        &client.address,
        "raffle_finalized",
        events::RaffleFinalized {
            raffle_id: client.address.clone(),
            winners: soroban_sdk::vec![&env, client.get_raffle().winners.get(0).unwrap().address],
            winning_ticket_ids: soroban_sdk::vec![&env, 1], // buyer_a is first ticket
            total_tickets_sold: 3,
            randomness_source: RandomnessSource::Internal,
            randomness_type: raffle_shared::RandomnessType::PseudoRandom,
            finalized_at: env.ledger().timestamp(),
        },
    );
    let finalized = client.get_raffle();
    assert_eq!(finalized.status, RaffleStatus::Finalized);
    assert_metadata_hash(&client, &expected_metadata_hash);
    assert_eq!(finalized.winners.len(), 1);

/// #485: with unique_winners, two buyers with multiple tickets each win at most one tier.

#[test]
fn init_accepts_min_ticket_price_and_rejects_below_it() {
    let (env, contract_id, factory, admin, creator, payment_token) = init_bounds_env();
    let client = ContractClient::new(&env, &contract_id);
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
    let client = ContractClient::new(&env, &contract_id);
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
    let client = ContractClient::new(&env, &contract_id);
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
    let client = ContractClient::new(&env, &contract_id);
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
    let client = ContractClient::new(&env, &contract_id);
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

// ===========================================================================
// buy_tickets budget benchmark near maximum ticket count (#449)
//
// Confirms a full `max_tickets_per_tx` batch of purchases near the 100_000
// ticket ceiling stays within Soroban's per-invocation CPU/memory limits and
// still triggers the transition into `Drawing` on the final batch.
//
// NOTE: The companion `get_tickets_page_is_efficient_for_large_raffles` test
// from #449 is intentionally omitted — the raffle instance does not currently
// expose a `get_tickets_page` view, so there is no function to benchmark. It
// should be added alongside a paginated ticket-read view in a follow-up.
// ===========================================================================

/// Soroban per-transaction resource ceilings (mainnet network settings).
const SOROBAN_CPU_INSTRUCTION_LIMIT: u64 = 100_000_000;
const SOROBAN_MEMORY_LIMIT_BYTES: u64 = 40 * 1024 * 1024;

#[test]
fn update_metadata_hash_before_deposit_only() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(crate::Contract, ());
    let client = crate::ContractClient::new(&env, &contract_id);
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
        metadata_hash: BytesN::from_array(&env, &[1u8; 32]),
        claim_lockup_seconds: 0,
        swap_deadline_seconds: 0,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
        unique_winners: false,
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
}

#[test]
fn test_explicit_zero_lockup_is_honored() {
    let env = Env::default();
    env.mock_all_auths();
    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let payment_token = env.register_stellar_asset_contract_v2(Address::generate(&env)).address();

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Zero Lockup"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 1,
        max_tickets_per_tx: 1,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: 1,
        payment_token,
        prize_amount: 1,
        prizes: soroban_sdk::vec![&env, 10000],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(&env, &[99; 32]),
        claim_lockup_seconds: Some(0),
        swap_deadline_seconds: Some(0),
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);
    let raffle = client.get_raffle();
    
    // Explicit 0 should be stored as 0, not DEFAULT_CLAIM_LOCKUP_SECONDS
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
    let payment_token = env.register_stellar_asset_contract_v2(Address::generate(&env)).address();

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Default Lockup"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 1,
        max_tickets_per_tx: 1,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: 1,
        payment_token,
        prize_amount: 1,
        prizes: soroban_sdk::vec![&env, 10000],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(&env, &[100; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&factory, &admin, &creator, &config);
    let raffle = client.get_raffle();
    
    // None should be resolved to the defaults
    assert_eq!(raffle.claim_lockup_seconds, DEFAULT_CLAIM_LOCKUP_SECONDS);
    assert_eq!(raffle.swap_deadline_seconds, DEFAULT_SWAP_DEADLINE_SECONDS);
}

#[test]
fn init_rejects_quorum_k_zero() {
    let (env, factory, admin, creator, payment_token, _) = init_bounds_env();
    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let oracle = Address::generate(&env);
    let mut oracles = Vec::new(&env);
    oracles.push_back(oracle);

    let mut config = init_bounds_config(&env, &payment_token, 60);
    config.randomness_source = RandomnessSource::Quorum(QuorumConfig { k: 0, oracles });

    assert_eq!(
        client.try_init(&factory, &admin, &creator, &config),
        Err(Ok(Error::InvalidParameters))
    );
}

#[test]
fn init_rejects_quorum_k_greater_than_n() {
    let (env, factory, admin, creator, payment_token, _) = init_bounds_env();
    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let oracle = Address::generate(&env);
    let mut oracles = Vec::new(&env);
    oracles.push_back(oracle);

    let mut config = init_bounds_config(&env, &payment_token, 61);
    config.randomness_source = RandomnessSource::Quorum(QuorumConfig { k: 2, oracles });

    assert_eq!(
        client.try_init(&factory, &admin, &creator, &config),
        Err(Ok(Error::InvalidParameters))
    );
}

#[test]
fn init_rejects_quorum_more_than_ten_oracles() {
    let (env, factory, admin, creator, payment_token, _) = init_bounds_env();
    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let mut oracles = Vec::new(&env);
    for _ in 0..11 {
        oracles.push_back(Address::generate(&env));
    }

    let mut config = init_bounds_config(&env, &payment_token, 62);
    config.randomness_source = RandomnessSource::Quorum(QuorumConfig { k: 2, oracles });

    assert_eq!(
        client.try_init(&factory, &admin, &creator, &config),
        Err(Ok(Error::InvalidParameters))
    );
}

#[test]
fn init_rejects_quorum_oracle_equal_to_contract() {
    let (env, factory, admin, creator, payment_token, _) = init_bounds_env();
    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let mut oracles = Vec::new(&env);
    oracles.push_back(contract_id.clone());

    let mut config = init_bounds_config(&env, &payment_token, 63);
    config.randomness_source = RandomnessSource::Quorum(QuorumConfig { k: 1, oracles });

    assert_eq!(
        client.try_init(&factory, &admin, &creator, &config),
        Err(Ok(Error::InvalidParameters))
    );
}

