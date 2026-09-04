//! Finalization, randomness modes (external fallback, commit-reveal, quorum), drawing lock lifecycle.

use super::*;

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

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(&env, &contract_id);

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
        unique_winners: false,
        metadata_hash: BytesN::from_array(&env, &[1; 32]),
        claim_lockup_seconds: None,
        swap_deadline_seconds: None,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
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
    let start_events = env.events().all().len();
    let result = client.try_trigger_randomness_fallback(&creator, &false);
    assert_eq!(env.events().all().len(), start_events);
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

        let contract_id = env.register(Contract, ());
        let client = ContractClient::new(&env, &contract_id);

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

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

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
        claim_lockup_seconds: 0,
        swap_deadline_seconds: 0,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();
    client.buy_tickets(&buyer_a, &1);

    let commit = [7u8; 32];
    client.submit_commit(&1, &BytesN::from_array(&env, &commit));

    // Simulate ownership transfer to validate commit persistence by ticket_id.
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

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

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
        claim_lockup_seconds: 0,
        swap_deadline_seconds: 0,
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

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

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
        claim_lockup_seconds: 0,
        swap_deadline_seconds: 0,
        bundles: soroban_sdk::vec![&env],
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();
    client.buy_tickets(&creator, &1);
    client.finalize_raffle();

    assert_drawing_lock_cleared(&env, &contract_id);
}

#[test]
fn drawing_lock_cleared_after_fallback_refund() {
    let env = Env::default();
    env.mock_all_auths();

    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let config = RaffleConfig {
        description: String::from_str(&env, "Claim lockup at bound"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 10,
        max_tickets_per_tx: 10,
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
            metadata_hash: BytesN::from_array(&env, &[50; 32]),
        claim_lockup_seconds: 0,
        swap_deadline_seconds: 0,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
    };

    client.init(&creator_factory_addr(&env), &admin, &creator, &config);
    let raffle = client.get_raffle();
    assert_eq!(raffle.claim_lockup_seconds, MAX_CLAIM_LOCKUP_SECONDS);
}

#[test]
fn drawing_lock_cleared_after_cancel_in_drawing_state() {
    let env = Env::default();
    env.mock_all_auths();

    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();

    let config = RaffleConfig {
        description: String::from_str(&env, "Claim lockup above bound"),
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
        claim_lockup_seconds: 0,
        swap_deadline_seconds: 0,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();
    client.buy_tickets(&creator, &1);
    client.cancel_raffle(&CancelReason::CreatorCancelled);

    assert_drawing_lock_cleared(&env, &contract_id);
}

#[test]
fn unique_winners_limits_one_tier_per_address() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let contract_id = env.register(crate::Contract, ());
    let client = crate::ContractClient::new(&env, &contract_id);
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

    let winner_balance_before = token_ro.balance(&winner);
    let start_events = env.events().all().len();
    let claimed = client.claim_prize(&winner, &0u32);
    
    assert_event(
        &env,
        &client.address,
        "prize_claimed",
        events::PrizeClaimed {
            winner: winner.clone(),
            tier_index: 0,
            payment_token: payment_token.clone(),
            gross_amount: prize_amount,
            net_amount: prize_amount,
            platform_fee: 0,
            claimed_at: env.ledger().timestamp(),
        },
    );
    assert_eq!(env.events().all().len(), start_events + 1);
    assert_metadata_hash(&client, &expected_metadata_hash);

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
        metadata_hash: BytesN::from_array(&env, &[42u8; 32]),
        claim_lockup_seconds: 0,
        swap_deadline_seconds: 0,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
        unique_winners: true,
    };

    // 10. Fully-claimed raffle transitions to Claimed.
    assert_eq!(client.get_raffle().status, RaffleStatus::Claimed);
    assert_metadata_hash(&client, &expected_metadata_hash);

    env.ledger().set_timestamp(5_000);
    client.finalize_raffle();

    let raffle = client.get_raffle();
    assert_eq!(raffle.winners.len(), 3);

    let mut count_a = 0u32;
    let mut count_b = 0u32;
    for i in 0..raffle.winners.len() {
        let w = raffle.winners.get(i).unwrap();
        if w.address == buyer_a {
            count_a += 1;
        }
        if w.address == buyer_b {
            count_b += 1;
        }
    }
    assert_eq!(count_a, 1);
    assert_eq!(count_b, 1);

    let fairness = client.get_fairness_data();
    assert!(fairness.unique_winners);
}

#[test]
fn quorum_k_minus_one_does_not_finalize() {
    let env = Env::default();
    env.mock_all_auths();

    let oracle_a = Address::generate(&env);
    let oracle_b = Address::generate(&env);
    let (client, _contract_id, _creator, request_id) =
        setup_quorum_drawing_raffle(&env, 2, &[oracle_a.clone(), oracle_b.clone()]);

    client.provide_quorum_randomness(&oracle_a, &111, &request_id);

    let raffle = client.get_raffle();
    assert_eq!(raffle.status, RaffleStatus::Drawing);
}

#[test]
fn quorum_k_th_submission_finalizes() {
    let env = Env::default();
    env.mock_all_auths();

    let oracle_a = Address::generate(&env);
    let oracle_b = Address::generate(&env);
    let (client, contract_id, _creator, request_id) =
        setup_quorum_drawing_raffle(&env, 2, &[oracle_a.clone(), oracle_b.clone()]);

    client.provide_quorum_randomness(&oracle_a, &111, &request_id);
    client.provide_quorum_randomness(&oracle_b, &222, &request_id);

    let raffle = client.get_raffle();
    assert_eq!(raffle.status, RaffleStatus::Finalized);

    env.as_contract(&contract_id, || {
        assert!(!env
            .storage()
            .persistent()
            .has(&DataKey::QuorumSubmittedOracles));
    });
}

#[test]
fn quorum_rejects_unregistered_oracle() {
    let env = Env::default();
    env.mock_all_auths();

    let oracle_a = Address::generate(&env);
    let oracle_b = Address::generate(&env);
    let stranger = Address::generate(&env);
    let (client, _, _, request_id) =
        setup_quorum_drawing_raffle(&env, 2, &[oracle_a, oracle_b]);

    assert_eq!(
        client.try_provide_quorum_randomness(&stranger, &999, &request_id),
        Err(Ok(Error::OracleNotRegistered))
    );
}

#[test]
fn quorum_rejects_duplicate_submission() {
    let env = Env::default();
    env.mock_all_auths();

    let oracle_a = Address::generate(&env);
    let oracle_b = Address::generate(&env);
    let (client, _, _, request_id) =
        setup_quorum_drawing_raffle(&env, 2, &[oracle_a.clone(), oracle_b]);

    client.provide_quorum_randomness(&oracle_a, &111, &request_id);
    assert_eq!(
        client.try_provide_quorum_randomness(&oracle_a, &222, &request_id),
        Err(Ok(Error::DuplicateOracleSubmission))
    );
}

#[test]
fn quorum_rejects_mismatched_request_id() {
    let env = Env::default();
    env.mock_all_auths();

    let oracle_a = Address::generate(&env);
    let oracle_b = Address::generate(&env);
    let (client, _, _, request_id) =
        setup_quorum_drawing_raffle(&env, 2, &[oracle_a.clone(), oracle_b]);

    assert_eq!(
        client.try_provide_quorum_randomness(&oracle_a, &111, &(request_id + 1)),
        Err(Ok(Error::InvalidParameters))
    );
}

#[test]
fn quorum_rejects_submission_before_drawing_lock() {
    let env = Env::default();
    env.mock_all_auths();

    let oracle_a = Address::generate(&env);
    let oracle_b = Address::generate(&env);
    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);
    let factory = env.register(MockFactory, ());
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let (token_addr, token_mint) = create_token(&env, &token_admin);
    token_mint.mint(&creator, &1_000_000);

    let mut oracles = Vec::new(&env);
    oracles.push_back(oracle_a.clone());
    oracles.push_back(oracle_b.clone());

    let config = RaffleConfig {
        description: String::from_str(&env, "quorum pre-lock"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 5,
        max_tickets_per_tx: 5,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: token_addr,
        prize_amount: MIN_TICKET_PRICE * 5,
        prizes: vec![&env, 10000u32],
        randomness_source: RandomnessSource::Quorum(QuorumConfig { k: 2, oracles }),
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(&env, &[56u8; 32]),
        claim_lockup_seconds: 0,
        swap_deadline_seconds: 0,
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();

    assert_eq!(
        client.try_provide_quorum_randomness(&oracle_a, &1, &1),
        Err(Ok(Error::InvalidStatus))
    );
}

#[test]
fn quorum_aggregation_is_order_independent() {
    let env = Env::default();
    env.mock_all_auths();

    let oracle_a = Address::generate(&env);
    let oracle_b = Address::generate(&env);
    let oracle_c = Address::generate(&env);

    let (client_ab, _, _, req_ab) =
        setup_quorum_drawing_raffle(&env, 3, &[oracle_a.clone(), oracle_b.clone(), oracle_c.clone()]);
    client_ab.provide_quorum_randomness(&oracle_a, &10, &req_ab);
    client_ab.provide_quorum_randomness(&oracle_c, &30, &req_ab);
    client_ab.provide_quorum_randomness(&oracle_b, &20, &req_ab);
    let seed_abc = client_ab.get_fairness_data().seed;

    let (client_ba, _, _, req_ba) =
        setup_quorum_drawing_raffle(&env, 3, &[oracle_a.clone(), oracle_b.clone(), oracle_c.clone()]);
    client_ba.provide_quorum_randomness(&oracle_b, &20, &req_ba);
    client_ba.provide_quorum_randomness(&oracle_a, &10, &req_ba);
    client_ba.provide_quorum_randomness(&oracle_c, &30, &req_ba);
    let seed_bac = client_ba.get_fairness_data().seed;

    assert_eq!(seed_abc, seed_bac);
}

#[test]
fn quorum_storage_cleared_allows_redraw() {
    let env = Env::default();
    env.mock_all_auths();

    let oracle_a = Address::generate(&env);
    let oracle_b = Address::generate(&env);
    let (client, contract_id, creator, request_id) =
        setup_quorum_drawing_raffle(&env, 2, &[oracle_a.clone(), oracle_b.clone()]);

    client.provide_quorum_randomness(&oracle_a, &111, &request_id);
    client.provide_quorum_randomness(&oracle_b, &222, &request_id);
    assert_eq!(client.get_raffle().status, RaffleStatus::Finalized);

    // Simulate a re-draw by re-arming drawing state (same oracles, new request).
    env.as_contract(&contract_id, || {
        let has_submitted = env.storage().persistent().has(&DataKey::QuorumSubmittedOracles);
        assert!(!has_submitted, "QuorumSubmittedOracles should be cleared after finalization");
        assert!(!env.storage().persistent().has(&DataKey::QuorumSeed(oracle_a.clone())), "QuorumSeed for oracle_a should be cleared");
        assert!(!env.storage().persistent().has(&DataKey::QuorumSeed(oracle_b.clone())), "QuorumSeed for oracle_b should be cleared");

        let mut raffle = crate::read_raffle(&env).unwrap();
        raffle.status = RaffleStatus::Drawing;
        crate::write_raffle(&env, &raffle);
        env.storage().instance().set(&DataKey::DrawingLock, &true);
        env.storage()
            .instance()
            .set(&DataKey::RandomnessRequestId, &999u64);
    });

    // Same oracles can submit again on the new request.
    client.provide_quorum_randomness(&oracle_a, &333, &999);
    assert_eq!(client.get_raffle().status, RaffleStatus::Drawing);
    client.provide_quorum_randomness(&oracle_b, &444, &999);
    assert_eq!(client.get_raffle().status, RaffleStatus::Finalized);
}

#[test]
fn test_unique_winners_single_owner_terminates() {
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
    let token_mint = StellarAssetClient::new(&env, &payment_token);
    token_mint.mint(&creator, &1_000_000);
    token_mint.mint(&buyer, &1_000_000);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "Single owner unique winners"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 5,
        max_tickets_per_tx: 5,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: payment_token.clone(),
        prize_amount: MIN_TICKET_PRICE * 5,
        prizes: soroban_sdk::vec![&env, 5000, 5000],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        unique_winners: true,
        metadata_hash: BytesN::from_array(&env, &[101; 32]),
        claim_lockup_seconds: Some(0),
        swap_deadline_seconds: Some(0),
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();
    client.buy_tickets(&buyer, &5);

    client.finalize_raffle();

    let raffle = client.get_raffle();
    assert_eq!(raffle.status, RaffleStatus::Finalized);
    assert_eq!(raffle.winners.len(), 2);
}

#[test]
fn test_unique_winners_n_distinct_owners_n_tiers() {
    let env = Env::default();
    env.mock_all_auths();

    let factory = Address::generate(&env);
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let buyer1 = Address::generate(&env);
    let buyer2 = Address::generate(&env);
    let buyer3 = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_mint = StellarAssetClient::new(&env, &payment_token);
    token_mint.mint(&creator, &1_000_000);
    token_mint.mint(&buyer1, &100_000);
    token_mint.mint(&buyer2, &100_000);
    token_mint.mint(&buyer3, &100_000);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let config = RaffleConfig {
        description: String::from_str(&env, "N distinct owners N tiers"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 3,
        max_tickets_per_tx: 1,
        min_tickets: 3,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: payment_token.clone(),
        prize_amount: MIN_TICKET_PRICE * 3,
        prizes: soroban_sdk::vec![&env, 3333, 3333, 3334],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        unique_winners: true,
        metadata_hash: BytesN::from_array(&env, &[102; 32]),
        claim_lockup_seconds: Some(0),
        swap_deadline_seconds: Some(0),
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();

    client.buy_tickets(&buyer1, &1);
    client.buy_tickets(&buyer2, &1);
    client.buy_tickets(&buyer3, &1);

    client.finalize_raffle();

    let raffle = client.get_raffle();
    assert_eq!(raffle.status, RaffleStatus::Finalized);
    assert_eq!(raffle.winners.len(), 3);

    let w0 = raffle.winners.get(0).unwrap();
    let w1 = raffle.winners.get(1).unwrap();
    let w2 = raffle.winners.get(2).unwrap();

    assert_ne!(w0, w1);
    assert_ne!(w0, w2);
    assert_ne!(w1, w2);
}
