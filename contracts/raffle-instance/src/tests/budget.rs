//! Instruction-budget regression tests (#831).
//!
//! Baselines are committed in `baselines.json`. Costs above baseline × (1 +
//! `TOLERANCE_FRACTION`) fail the test.

use raffle_shared::constants::{MAX_PRIZES, MAX_TICKETS_LIMIT};
use soroban_sdk::{
    testutils::budget::Budget,
    token::StellarAssetClient,
    Address, BytesN, Env, String, Vec,
};

use crate::{
    read_raffle, DataKey, Error, RaffleConfig, RaffleInstance, RaffleStatus, RandomnessSource,
    MIN_TICKET_PRICE,
};

const TOLERANCE_FRACTION: f64 = 0.10;

const BUY_TICKETS_MAX_BATCH: Baseline = Baseline {
    cpu_instructions: 30_000_000,
    memory_bytes: 10 * 1024 * 1024,
};
const FINALIZE_MAX_PRIZES: Baseline = Baseline {
    cpu_instructions: 80_000_000,
    memory_bytes: 24 * 1024 * 1024,
};
const SWEEP_UNCLAIMED_MAX_TIERS: Baseline = Baseline {
    cpu_instructions: 50_000_000,
    memory_bytes: 20 * 1024 * 1024,
};
const EXTEND_TTL_MAX_TICKETS: Baseline = Baseline {
    cpu_instructions: 40_000_000,
    memory_bytes: 16 * 1024 * 1024,
};

#[derive(Clone, Copy)]
struct Baseline {
    cpu_instructions: u64,
    memory_bytes: u64,
}

#[derive(Debug, Clone, Copy)]
struct Snapshot {
    cpu: u64,
    mem: u64,
}

fn measure<F: FnOnce()>(env: &Env, f: F) -> Snapshot {
    env.cost_estimate().budget().reset_default();
    f();
    let budget = env.cost_estimate().budget();
    Snapshot {
        cpu: budget.cpu_instruction_cost(),
        mem: budget.memory_bytes_cost(),
    }
}

fn assert_within_tolerance(label: &str, actual: Snapshot, baseline: Baseline) {
    let cpu_limit =
        ((baseline.cpu_instructions as f64) * (1.0 + TOLERANCE_FRACTION)).ceil() as u64;
    let mem_limit = ((baseline.memory_bytes as f64) * (1.0 + TOLERANCE_FRACTION)).ceil() as u64;

    assert!(
        actual.cpu <= cpu_limit,
        "{label}: cpu {} exceeded baseline {} + {:.0}% (limit {})",
        actual.cpu,
        baseline.cpu_instructions,
        TOLERANCE_FRACTION * 100.0,
        cpu_limit
    );
    assert!(
        actual.mem <= mem_limit,
        "{label}: memory {} exceeded baseline {} + {:.0}% (limit {})",
        actual.mem,
        baseline.memory_bytes,
        TOLERANCE_FRACTION * 100.0,
        mem_limit
    );
}

fn build_prizes(env: &Env, tiers: u32) -> Vec<u32> {
    let each = 10_000u32 / tiers;
    let mut prizes = Vec::new(env);
    let mut sum = 0u32;
    for i in 0..tiers {
        if i + 1 == tiers {
            prizes.push_back(10_000 - sum);
        } else {
            prizes.push_back(each);
            sum += each;
        }
    }
    prizes
}

fn setup_raffle(
    env: &Env,
    max_tickets: u32,
    max_tickets_per_tx: u32,
    prize_tiers: u32,
) -> (RaffleInstanceClient<'_>, Address, Address) {
    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(env, &contract_id);
    let factory = Address::generate(env);
    let admin = Address::generate(env);
    let creator = Address::generate(env);
    let buyer = Address::generate(env);

    let token_admin = Address::generate(env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let token = StellarAssetClient::new(env, &payment_token);
    token.mint(&creator, &1_000_000_000_000);
    token.mint(&buyer, &1_000_000_000_000);

    let config = RaffleConfig {
        description: String::from_str(env, "budget"),
        end_time: 0,
        no_deadline: true,
        max_tickets,
        max_tickets_per_tx,
        max_tickets_per_address: 0,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: payment_token.clone(),
        prize_amount: MIN_TICKET_PRICE * max_tickets as i128,
        prizes: build_prizes(env, prize_tiers),
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: Some(Address::generate(env)),
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(env, &[9u8; 32]),
        claim_lockup_seconds: Some(0),
        claim_expiry_seconds: None,
        swap_deadline_seconds: Some(300),
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
        unique_winners: false,
        bundles: Vec::new(env),
        prize_token: None,
        nft_contract: None,
    };

    client.init(&factory, &admin, &creator, &config);
    env.as_contract(&contract_id, || {
        env.storage().instance().remove(&DataKey::Factory);
    });
    client.deposit_prize();

    (client, contract_id, buyer)
}

#[test]
fn buy_tickets_at_max_batch_within_baseline() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, buyer) = setup_raffle(&env, MAX_TICKETS_LIMIT, 1_000, 1);
    let snap = measure(&env, || {
        client.buy_tickets(&buyer, &1_000);
    });
    assert_within_tolerance("buy_tickets_max_batch", snap, BUY_TICKETS_MAX_BATCH);
}

#[test]
fn finalize_max_prizes_and_tickets_within_baseline() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, contract_id, buyer) =
        setup_raffle(&env, MAX_TICKETS_LIMIT, 1_000, MAX_PRIZES);

    for _ in 0..(MAX_TICKETS_LIMIT / 1_000) {
        client.buy_tickets(&buyer, &1_000);
    }

    let snap = measure(&env, || {
        client.finalize_raffle();
    });
    assert_within_tolerance("finalize_max_prizes", snap, FINALIZE_MAX_PRIZES);

    env.as_contract(&contract_id, || {
        let raffle = read_raffle(&env).unwrap();
        assert_eq!(raffle.status, RaffleStatus::Finalized);
        assert_eq!(raffle.tickets_sold, MAX_TICKETS_LIMIT);
        assert_eq!(raffle.prizes.len(), MAX_PRIZES as u32);
    });
}

#[test]
fn sweep_unclaimed_max_tiers_within_baseline() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, contract_id, buyer) = setup_raffle(&env, MAX_PRIZES, MAX_PRIZES, MAX_PRIZES);
    for _ in 0..MAX_PRIZES {
        client.buy_tickets(&buyer, &1);
    }
    client.finalize_raffle();

    let raffle = env.as_contract(&contract_id, || read_raffle(&env).unwrap());
    let sweep_after = raffle.finalized_at.unwrap() + raffle.claim_expiry_seconds + 1;
    env.ledger().set_timestamp(sweep_after);

    let mut start = 0u32;
    let page = raffle_shared::constants::MAX_SWEEP_UNCLAIMED_PER_CALL;
    let mut total_swept = 0u32;
    let snap = measure(&env, || {
        loop {
            let result = client.try_sweep_unclaimed(&start, &page);
            assert_ne!(result, Err(Ok(Error::ClaimTooEarly)));
            assert_ne!(result, Err(Ok(Error::InvalidStatus)));
            let swept = result.unwrap().unwrap();
            if swept == 0 {
                break;
            }
            total_swept += swept;
            start = start.saturating_add(page);
        }
    });
    assert_eq!(total_swept, MAX_PRIZES);
    assert_within_tolerance("sweep_unclaimed_max_tiers", snap, SWEEP_UNCLAIMED_MAX_TIERS);
}

#[test]
fn sweep_unclaimed_before_expiry_returns_claim_too_early() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, contract_id, buyer) = setup_raffle(&env, 10, 10, 1);
    client.buy_tickets(&buyer, &1);
    client.finalize_raffle();

    let raffle = env.as_contract(&contract_id, || read_raffle(&env).unwrap());
    let too_early = raffle.finalized_at.unwrap() + raffle.claim_expiry_seconds - 1;
    env.ledger().set_timestamp(too_early);

    assert_eq!(
        client.try_sweep_unclaimed(&0, &10),
        Err(Ok(Error::ClaimTooEarly))
    );
}

#[test]
fn extend_ttl_max_tickets_within_baseline() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, buyer) = setup_raffle(&env, 1_000, 1_000, 1);
    client.buy_tickets(&buyer, &1_000);

    let snap = measure(&env, || {
        let _ = client.try_extend_ttl();
    });
    assert_within_tolerance("extend_ttl_max_tickets", snap, EXTEND_TTL_MAX_TICKETS);
}
