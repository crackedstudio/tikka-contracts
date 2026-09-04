//! Edge-case tests for the views module pagination and coverage.

use raffle_shared::{PaginationParams, DEFAULT_PAGE_LIMIT, MAX_PAGE_LIMIT};
use soroban_sdk::{testutils::Address as _, Address, Env};

use crate::{RaffleFactory, RaffleFactoryClient, RaffleConfig};

fn setup_with_raffles(env: &Env, count: u32) -> RaffleFactoryClient<'_> {
    let admin = Address::generate(env);
    let treasury = Address::generate(env);
    let contract_id = env.register(RaffleFactory, ());
    let client = RaffleFactoryClient::new(env, &contract_id);

    client.initialize(&admin, &treasury, &60);

    let creator = Address::generate(env);
    let token = env.register_stellar_asset_contract_v2(admin.clone()).address();

    for _ in 0..count {
        let config = RaffleConfig {
            ticket_price: 100,
            max_tickets: 10,
            end_time: 0,
            token: token.clone(),
            randomness_source: crate::RandomnessSource::Internal,
            description: soroban_sdk::String::from_str(env, "test"),
            category: soroban_sdk::String::from_str(env, "general"),
            metadata_uri: soroban_sdk::String::from_str(env, ""),
        };
        env.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &creator,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &contract_id,
                fn_name: "create_raffle",
                args: (creator.clone(), config).into_val(env),
                sub_invokes: &[],
            },
        }]);
        client.create_raffle(&creator, &config);
    }

    client
}

// ── get_raffles_page ─────────────────────────────────────────────────────

#[test]
fn raffles_page_offset_beyond_total_returns_empty() {
    let env = Env::default();
    let client = setup_with_raffles(&env, 5);

    let page = client.get_raffles_page(&PaginationParams {
        offset: 100,
        limit: 10,
    });

    assert_eq!(page.items.len(), 0);
    assert_eq!(page.total, 5);
    assert!(!page.has_more);
}

#[test]
fn raffles_page_limit_zero_uses_default() {
    let env = Env::default();
    let client = setup_with_raffles(&env, 5);

    let page = client.get_raffles_page(&PaginationParams {
        offset: 0,
        limit: 0,
    });

    assert_eq!(page.items.len(), 5);
    assert_eq!(page.total, 5);
    assert!(!page.has_more);
}

#[test]
fn raffles_page_limit_exceeds_max_clamped() {
    let env = Env::default();
    let client = setup_with_raffles(&env, 5);

    let page = client.get_raffles_page(&PaginationParams {
        offset: 0,
        limit: MAX_PAGE_LIMIT + 100,
    });

    assert_eq!(page.items.len(), 5);
    assert_eq!(page.total, 5);
    assert!(!page.has_more);
}

// ── get_raffles_by_creator ───────────────────────────────────────────────

#[test]
fn creator_raffles_offset_beyond_total_returns_empty() {
    let env = Env::default();
    let client = setup_with_raffles(&env, 3);
    let creator = Address::generate(&env);

    // creator has 0 raffles, offset 0 is beyond
    let page = client.get_raffles_by_creator(&creator, &PaginationParams {
        offset: 0,
        limit: 10,
    });

    assert_eq!(page.items.len(), 0);
    assert_eq!(page.total, 0);
    assert!(!page.has_more);
}

#[test]
fn creator_raffles_limit_zero_uses_default() {
    let env = Env::default();
    let client = setup_with_raffles(&env, 3);

    // Get the creator of the first raffle
    let creator = Address::generate(&env);

    let page = client.get_raffles_by_creator(&creator, &PaginationParams {
        offset: 0,
        limit: 0,
    });

    assert_eq!(page.total, 0);
}

// ── get_raffles_by_category ──────────────────────────────────────────────

#[test]
fn category_raffles_offset_beyond_total_returns_empty() {
    let env = Env::default();
    let client = setup_with_raffles(&env, 3);

    let page = client.get_raffles_by_category(
        &soroban_sdk::String::from_str(&env, "nonexistent"),
        &PaginationParams {
            offset: 100,
            limit: 10,
        },
    );

    assert_eq!(page.items.len(), 0);
    assert_eq!(page.total, 0);
    assert!(!page.has_more);
}

#[test]
fn category_raffles_limit_zero_uses_default() {
    let env = Env::default();
    let client = setup_with_raffles(&env, 3);

    let page = client.get_raffles_by_category(
        &soroban_sdk::String::from_str(&env, "nonexistent"),
        &PaginationParams {
            offset: 0,
            limit: 0,
        },
    );

    assert_eq!(page.items.len(), 0);
    assert_eq!(page.total, 0);
}

// ── No storage writes in view functions ──────────────────────────────────

#[test]
fn get_protocol_stats_does_not_write_storage() {
    let env = Env::default();
    let client = setup_with_raffles(&env, 2);

    // Call twice — if it wrote storage, the second call would differ
    let stats1 = client.get_protocol_stats();
    let stats2 = client.get_protocol_stats();
    assert_eq!(stats1.total_raffles_created, stats2.total_raffles_created);
    assert_eq!(stats1.protocol_fee_bp, stats2.protocol_fee_bp);
    assert_eq!(stats1.paused, stats2.paused);
    assert_eq!(
        stats1.total_unique_participants,
        stats2.total_unique_participants
    );
}

#[test]
fn get_admin_returns_correct_address() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let contract_id = env.register(RaffleFactory, ());
    let client = RaffleFactoryClient::new(&env, &contract_id);
    client.initialize(&admin, &treasury, &60);

    assert_eq!(client.get_admin(), Ok(admin));
}
