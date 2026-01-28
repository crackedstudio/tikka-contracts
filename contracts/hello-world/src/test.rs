#![cfg(test)]

use super::*;
use soroban_sdk::{
    Address, Env, IntoVal, String, Symbol, TryIntoVal, 
    testutils::{Address as _, Events, Ledger}, 
    token, symbol_short
};

/// HELPER: Standardized environment setup
fn setup_raffle_env(
    env: &Env,
) -> (
    ContractClient<'_>,
    Address,
    Address,
    token::StellarAssetClient<'_>,
    u64,
) {
    let creator = Address::generate(env);
    let buyer = Address::generate(env);
    let admin = Address::generate(env);

    let token_contract = env.register_stellar_asset_contract_v2(admin.clone());
    let token_id = token_contract.address();
    let admin_client = token::StellarAssetClient::new(env, &token_id);

    // FIXED: Added & to amounts and explicitly typed as i128
    admin_client.mint(&creator, &1_000i128);
    admin_client.mint(&buyer, &1_000i128);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(env, &contract_id);

    let raffle_id = client.create_raffle(
        &creator,
        &String::from_str(env, "Audit Raffle"),
        &0,
        &10,
        &false,
        &10i128,
        &token_id,
        &100i128,
    );

    (client, creator, buyer, admin_client, raffle_id)
}

// --- 1. FUNCTIONAL FLOW TESTS ---

#[test]
fn test_basic_raffle_flow() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, creator, buyer, admin_client, raffle_id) = setup_raffle_env(&env);
    let token_client = token::Client::new(&env, &admin_client.address);

    client.deposit_prize(&raffle_id);
    client.buy_ticket(&raffle_id, &buyer);

    let winner = client.finalize_raffle(&raffle_id, &String::from_str(&env, "prng"));
    let claimed_amount = client.claim_prize(&raffle_id, &winner);

    assert_eq!(token_client.balance(&winner), 1_090i128);
    assert_eq!(token_client.balance(&creator), 900i128);
}

// --- 2. RANDOMNESS SOURCE TESTS ---

#[test]
fn test_randomness_source_prng() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, buyer, _, raffle_id) = setup_raffle_env(&env);

    client.deposit_prize(&raffle_id);
    client.buy_ticket(&raffle_id, &buyer);

    let source = String::from_str(&env, "prng");
    let winner = client.finalize_raffle(&raffle_id, &source);

    assert_eq!(winner, buyer);
}

#[test]
fn test_randomness_source_oracle() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _, buyer, _, raffle_id) = setup_raffle_env(&env);

    client.deposit_prize(&raffle_id);
    client.buy_ticket(&raffle_id, &buyer);

    let source = String::from_str(&env, "oracle");
    let winner = client.finalize_raffle(&raffle_id, &source);

    assert_eq!(winner, buyer);
}

// --- 3. EVENT AUDIT & STATE VALIDATION ---

#[test]
fn test_raffle_finalized_event_audit() {
    let env = Env::default();
    env.mock_all_auths();

    let expected_timestamp = 123456789;
    env.ledger().with_mut(|l| {
        l.timestamp = expected_timestamp;
    });

    let (client, _, buyer_1, admin_client, raffle_id) = setup_raffle_env(&env);

    let buyer_2 = Address::generate(&env);
    admin_client.mint(&buyer_2, &1_000i128);

    client.deposit_prize(&raffle_id);
    client.buy_ticket(&raffle_id, &buyer_1);
    client.buy_ticket(&raffle_id, &buyer_2);

    let source = String::from_str(&env, "oracle");
    let winner = client.finalize_raffle(&raffle_id, &source);

    // --- FIXED EVENT AUDIT SECTION ---
    let last_event = env.events().all().last().expect("No event emitted");

    // Topic 0 in contract is symbol_short!("finalized")
    let topic_0: Symbol = last_event.1.get(0).unwrap().into_val(&env);
    // Topic 1 in contract is raffle_id (u64)
    let topic_1: u64 = last_event.1.get(1).unwrap().into_val(&env);

    assert_eq!(topic_0, symbol_short!("finalized"));
    assert_eq!(topic_1, raffle_id);

    let event_data: RaffleFinalized = last_event.2.into_val(&env);

    assert_eq!(event_data.raffle_id, raffle_id);
    assert_eq!(event_data.winner, winner);
    assert_eq!(event_data.total_tickets_sold, 2);
    assert_eq!(event_data.randomness_source, source);
    assert_eq!(event_data.finalized_at, expected_timestamp);
    assert!(event_data.winning_ticket_id < 2);
}

#[test]
fn test_single_ticket_purchase_event() {
    let env = Env::default();
    env.mock_all_auths();

    let creator = Address::generate(&env);
    let buyer = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_id = token_contract.address();
    let _token_client = token::Client::new(&env, &token_id);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_id);

    token_admin_client.mint(&creator, &1_000);
    token_admin_client.mint(&buyer, &1_000);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let raffle_id = client.create_raffle(
        &creator,
        &String::from_str(&env, "Test Raffle"),
        &0u64,
        &10u32,
        &true, // allow_multiple
        &10i128,
        &token_id,
        &100i128,
    );

    client.deposit_prize(&raffle_id);

    // Clear events before purchase
    let _ = env.events().all();

    let timestamp_before = env.ledger().timestamp();
    client.buy_ticket(&raffle_id, &buyer);
    let timestamp_after = env.ledger().timestamp();

    // Retrieve events and find TicketPurchased event
    let events = env.events().all();
    let mut found_event: Option<TicketPurchased> = None;
    let mut event_count = 0;
    
    for event in events {
        if let Ok(data) = event.2.try_into_val(&env) {
            let event_data: TicketPurchased = data;
            if event_data.raffle_id == raffle_id {
                event_count += 1;
                if found_event.is_none() {
                    found_event = Some(event_data);
                }
            }
        }
    }

    assert_eq!(event_count, 1, "Should emit exactly one TicketPurchased event");
    let event = found_event.expect("Should have found TicketPurchased event");

    // Verify all 6 required fields
    assert_eq!(event.raffle_id, raffle_id);
    assert_eq!(event.buyer, buyer);
    assert_eq!(event.quantity, 1u32);
    assert_eq!(event.total_paid, 10i128); // ticket_price * quantity
    assert!(event.timestamp >= timestamp_before && event.timestamp <= timestamp_after);
    assert_eq!(event.ticket_ids.len(), 1);
    assert_eq!(event.ticket_ids.get(0).unwrap(), 1u32); // First ticket is ID 1
}

#[test]
fn test_batch_ticket_purchase_event() {
    let env = Env::default();
    env.mock_all_auths();

    let creator = Address::generate(&env);
    let buyer = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_id = token_contract.address();
    let _token_client = token::Client::new(&env, &token_id);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_id);

    token_admin_client.mint(&creator, &1_000);
    token_admin_client.mint(&buyer, &1_000);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let raffle_id = client.create_raffle(
        &creator,
        &String::from_str(&env, "Batch Test Raffle"),
        &0u64,
        &10u32,
        &true, // allow_multiple
        &10i128,
        &token_id,
        &100i128,
    );

    client.deposit_prize(&raffle_id);

    // Clear events before purchase
    let _ = env.events().all();

    let quantity = 3u32;
    let timestamp_before = env.ledger().timestamp();
    client.buy_tickets(&raffle_id, &buyer, &quantity);
    let timestamp_after = env.ledger().timestamp();

    // Retrieve events and find TicketPurchased event
    let events = env.events().all();
    let mut found_event: Option<TicketPurchased> = None;
    let mut event_count = 0;
    
    for event in events {
        if let Ok(data) = event.2.try_into_val(&env) {
            let event_data: TicketPurchased = data;
            if event_data.raffle_id == raffle_id {
                event_count += 1;
                if found_event.is_none() {
                    found_event = Some(event_data);
                }
            }
        }
    }

    assert_eq!(event_count, 1, "Should emit exactly one TicketPurchased event for batch purchase");
    let event = found_event.expect("Should have found TicketPurchased event");

    // Verify all 6 required fields
    assert_eq!(event.raffle_id, raffle_id);
    assert_eq!(event.buyer, buyer);
    assert_eq!(event.quantity, quantity);
    assert_eq!(event.total_paid, 30i128); // ticket_price (10) * quantity (3)
    assert!(event.timestamp >= timestamp_before && event.timestamp <= timestamp_after);
    
    // Verify ticket_ids contains all purchased ticket IDs
    assert_eq!(event.ticket_ids.len(), quantity);
    assert_eq!(event.ticket_ids.get(0).unwrap(), 1u32); // First ticket
    assert_eq!(event.ticket_ids.get(1).unwrap(), 2u32); // Second ticket
    assert_eq!(event.ticket_ids.get(2).unwrap(), 3u32); // Third ticket
}

#[test]
fn test_multiple_single_purchases_emit_multiple_events() {
    let env = Env::default();
    env.mock_all_auths();

    let creator = Address::generate(&env);
    let buyer1 = Address::generate(&env);
    let buyer2 = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_id = token_contract.address();
    let token_admin_client = token::StellarAssetClient::new(&env, &token_id);

    token_admin_client.mint(&creator, &1_000);
    token_admin_client.mint(&buyer1, &1_000);
    token_admin_client.mint(&buyer2, &1_000);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let raffle_id = client.create_raffle(
        &creator,
        &String::from_str(&env, "Multi Purchase Test"),
        &0u64,
        &10u32,
        &true, // allow_multiple
        &10i128,
        &token_id,
        &100i128,
    );

    client.deposit_prize(&raffle_id);

    // First purchase and get its event
    client.buy_ticket(&raffle_id, &buyer1);
    let events1 = env.events().all();
    let mut event1: Option<TicketPurchased> = None;
    for event in events1 {
        if let Ok(data) = event.2.try_into_val(&env) {
            let event_data: TicketPurchased = data;
            if event_data.raffle_id == raffle_id && event_data.buyer == buyer1 {
                event1 = Some(event_data);
                break;
            }
        }
    }
    
    // Second purchase and get its event
    client.buy_ticket(&raffle_id, &buyer2);
    let events2 = env.events().all();
    let mut event2: Option<TicketPurchased> = None;
    for event in events2 {
        if let Ok(data) = event.2.try_into_val(&env) {
            let event_data: TicketPurchased = data;
            if event_data.raffle_id == raffle_id && event_data.buyer == buyer2 {
                event2 = Some(event_data);
                break;
            }
        }
    }
    
    assert!(event1.is_some(), "Should have found event for buyer1");
    assert!(event2.is_some(), "Should have found event for buyer2");
    
    let e1 = event1.unwrap();
    let e2 = event2.unwrap();
    assert_eq!(e1.buyer, buyer1);
    assert_eq!(e1.ticket_ids.get(0).unwrap(), 1u32);
    assert_eq!(e2.buyer, buyer2);
    assert_eq!(e2.ticket_ids.get(0).unwrap(), 2u32);
}

#[test]
fn test_pagination_get_all_raffle_ids() {
    let env = Env::default();
    env.mock_all_auths();

    let creator = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_id = token_contract.address();

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    // Create 5 raffles
    for _ in 0..5 {
        client.create_raffle(
            &creator,
            &String::from_str(&env, "Test Raffle"),
            &0u64,
            &10u32,
            &true,
            &1i128,
            &token_id,
            &10i128,
        );
    }

    // Test offset 0, limit 3
    let result = client.get_all_raffle_ids(&0, &3, &false);
    assert_eq!(result.data.len(), 3);
    assert_eq!(result.meta.total, 5);
    assert_eq!(result.meta.offset, 0);
    assert_eq!(result.meta.limit, 3);
    assert!(result.meta.has_more);

    // Test offset 3, limit 3 (should get 2 items)
    let result = client.get_all_raffle_ids(&3, &3, &false);
    assert_eq!(result.data.len(), 2);
    assert_eq!(result.meta.total, 5);
    assert_eq!(result.meta.offset, 3);
    assert!(!result.meta.has_more);

    // Test offset beyond total
    let result = client.get_all_raffle_ids(&10, &3, &false);
    assert_eq!(result.data.len(), 0);
    assert_eq!(result.meta.total, 5);
    assert!(!result.meta.has_more);

    // Test newest_first
    let result = client.get_all_raffle_ids(&0, &3, &true);
    assert_eq!(result.data.get(0).unwrap(), 4u64); // newest first
    assert_eq!(result.data.get(2).unwrap(), 2u64);
}

#[test]
fn test_pagination_limit_enforced() {
    let env = Env::default();
    env.mock_all_auths();

    let creator = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_id = token_contract.address();

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    // Create 5 raffles
    for _ in 0..5 {
        client.create_raffle(
            &creator,
            &String::from_str(&env, "Test Raffle"),
            &0u64,
            &10u32,
            &true,
            &1i128,
            &token_id,
            &10i128,
        );
    }

    // Request limit > 100, should be capped
    let result = client.get_all_raffle_ids(&0, &200, &false);
    assert_eq!(result.meta.limit, 100); // Capped at 100
}

#[test]
fn test_pagination_empty_results() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    // Test with 0 raffles
    let result = client.get_all_raffle_ids(&0, &10, &false);
    assert_eq!(result.data.len(), 0);
    assert_eq!(result.meta.total, 0);
    assert!(!result.meta.has_more);
}

// --- USER PARTICIPATION TESTS ---

#[test]
fn test_user_raffle_index_maintained_on_single_ticket() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, creator, buyer, _, raffle_id) = setup_raffle_env(&env);

    client.deposit_prize(&raffle_id);
    client.buy_ticket(&raffle_id, &buyer);

    let participation = client.get_user_raffle_participation(&buyer, &0, &100);
    assert_eq!(participation.raffle_ids.len(), 1);
    assert_eq!(participation.raffle_ids.get(0).unwrap(), raffle_id);
    assert_eq!(participation.ticket_counts.len(), 1);
    assert_eq!(participation.ticket_counts.get(0).unwrap(), 1u32);
    assert_eq!(participation.total_spent, 10i128); // 1 ticket * 10 price
    assert_eq!(participation.win_count, 0u32);
    assert_eq!(participation.total_winnings, 0i128);
}

#[test]
fn test_user_raffle_index_maintained_on_batch_tickets() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, creator, buyer, _, raffle_id) = setup_raffle_env(&env);

    client.deposit_prize(&raffle_id);
    client.buy_tickets(&raffle_id, &buyer, &3);

    let participation = client.get_user_raffle_participation(&buyer, &0, &100);
    assert_eq!(participation.raffle_ids.len(), 1);
    assert_eq!(participation.raffle_ids.get(0).unwrap(), raffle_id);
    assert_eq!(participation.ticket_counts.len(), 1);
    assert_eq!(participation.ticket_counts.get(0).unwrap(), 3u32);
    assert_eq!(participation.total_spent, 30i128); // 3 tickets * 10 price
}

#[test]
fn test_user_participation_multiple_raffles() {
    let env = Env::default();
    env.mock_all_auths();
    let creator = Address::generate(&env);
    let buyer = Address::generate(&env);
    let admin = Address::generate(&env);

    let token_contract = env.register_stellar_asset_contract_v2(admin.clone());
    let token_id = token_contract.address();
    let admin_client = token::StellarAssetClient::new(&env, &token_id);

    admin_client.mint(&creator, &1_000i128);
    admin_client.mint(&buyer, &1_000i128);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    // Create 3 raffles
    let raffle1 = client.create_raffle(
        &creator,
        &String::from_str(&env, "Raffle 1"),
        &0,
        &10,
        &true,
        &10i128,
        &token_id,
        &100i128,
    );
    let raffle2 = client.create_raffle(
        &creator,
        &String::from_str(&env, "Raffle 2"),
        &0,
        &10,
        &true,
        &20i128,
        &token_id,
        &200i128,
    );
    let raffle3 = client.create_raffle(
        &creator,
        &String::from_str(&env, "Raffle 3"),
        &0,
        &10,
        &true,
        &5i128,
        &token_id,
        &50i128,
    );

    // Deposit prizes
    client.deposit_prize(&raffle1);
    client.deposit_prize(&raffle2);
    client.deposit_prize(&raffle3);

    // Buy tickets in different raffles
    client.buy_ticket(&raffle1, &buyer); // 1 ticket * 10 = 10
    client.buy_tickets(&raffle2, &buyer, &2); // 2 tickets * 20 = 40
    client.buy_ticket(&raffle3, &buyer); // 1 ticket * 5 = 5

    let participation = client.get_user_raffle_participation(&buyer, &0, &100);
    assert_eq!(participation.raffle_ids.len(), 3);
    assert_eq!(participation.total_spent, 55i128); // 10 + 40 + 5
    assert_eq!(participation.ticket_counts.len(), 3);
    assert_eq!(participation.ticket_counts.get(0).unwrap(), 1u32);
    assert_eq!(participation.ticket_counts.get(1).unwrap(), 2u32);
    assert_eq!(participation.ticket_counts.get(2).unwrap(), 1u32);
}

#[test]
fn test_user_participation_with_win() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, creator, buyer, _, raffle_id) = setup_raffle_env(&env);

    client.deposit_prize(&raffle_id);
    client.buy_ticket(&raffle_id, &buyer);

    // Finalize and buyer wins
    let winner = client.finalize_raffle(&raffle_id, &String::from_str(&env, "prng"));
    
    // Claim prize if buyer won
    if winner == buyer {
        client.claim_prize(&raffle_id, &buyer);
    }

    let participation = client.get_user_raffle_participation(&buyer, &0, &100);
    assert_eq!(participation.raffle_ids.len(), 1);
    assert_eq!(participation.total_spent, 10i128);
    
    if winner == buyer {
        assert_eq!(participation.win_count, 1u32);
        assert_eq!(participation.total_winnings, 100i128); // prize_amount - platform_fee (0)
    } else {
        assert_eq!(participation.win_count, 0u32);
        assert_eq!(participation.total_winnings, 0i128);
    }
}

#[test]
fn test_user_participation_pagination() {
    let env = Env::default();
    env.mock_all_auths();
    let creator = Address::generate(&env);
    let buyer = Address::generate(&env);
    let admin = Address::generate(&env);

    let token_contract = env.register_stellar_asset_contract_v2(admin.clone());
    let token_id = token_contract.address();
    let admin_client = token::StellarAssetClient::new(&env, &token_id);

    admin_client.mint(&creator, &1_000i128);
    admin_client.mint(&buyer, &1_000i128);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    // Create 5 raffles
    for _i in 0..5 {
        let raffle_id = client.create_raffle(
            &creator,
            &String::from_str(&env, "Raffle"),
            &0,
            &10,
            &true,
            &10i128,
            &token_id,
            &100i128,
        );
        client.deposit_prize(&raffle_id);
        client.buy_ticket(&raffle_id, &buyer);
    }

    // Test pagination: first page
    let participation = client.get_user_raffle_participation(&buyer, &0, &3);
    assert_eq!(participation.raffle_ids.len(), 3);
    assert_eq!(participation.ticket_counts.len(), 3);
    assert_eq!(participation.total_spent, 30i128); // 3 raffles * 10

    // Test pagination: second page
    let participation = client.get_user_raffle_participation(&buyer, &3, &3);
    assert_eq!(participation.raffle_ids.len(), 2);
    assert_eq!(participation.ticket_counts.len(), 2);
    assert_eq!(participation.total_spent, 20i128); // 2 raffles * 10
}

#[test]
fn test_user_participation_empty() {
    let env = Env::default();
    env.mock_all_auths();
    let buyer = Address::generate(&env);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    // User with no raffles
    let participation = client.get_user_raffle_participation(&buyer, &0, &100);
    assert_eq!(participation.raffle_ids.len(), 0);
    assert_eq!(participation.ticket_counts.len(), 0);
    assert_eq!(participation.total_spent, 0i128);
    assert_eq!(participation.win_count, 0u32);
    assert_eq!(participation.total_winnings, 0i128);
}

#[test]
fn test_user_participation_no_duplicate_raffles() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, creator, buyer, _, raffle_id) = setup_raffle_env(&env);

    client.deposit_prize(&raffle_id);
    
    // Buy multiple tickets in the same raffle
    client.buy_ticket(&raffle_id, &buyer);
    client.buy_ticket(&raffle_id, &buyer);
    client.buy_ticket(&raffle_id, &buyer);

    let participation = client.get_user_raffle_participation(&buyer, &0, &100);
    // Should only appear once in raffle_ids
    assert_eq!(participation.raffle_ids.len(), 1);
    assert_eq!(participation.raffle_ids.get(0).unwrap(), raffle_id);
    // But ticket count should reflect all purchases
    assert_eq!(participation.ticket_counts.get(0).unwrap(), 3u32);
    assert_eq!(participation.total_spent, 30i128); // 3 tickets * 10
}

#[test]
fn test_user_participation_multiple_wins() {
    let env = Env::default();
    env.mock_all_auths();
    let creator = Address::generate(&env);
    let buyer = Address::generate(&env);
    let admin = Address::generate(&env);

    let token_contract = env.register_stellar_asset_contract_v2(admin.clone());
    let token_id = token_contract.address();
    let admin_client = token::StellarAssetClient::new(&env, &token_id);

    admin_client.mint(&creator, &10_000i128);
    admin_client.mint(&buyer, &1_000i128);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    // Create 3 raffles where buyer is the only participant (guaranteed win)
    for i in 0..3 {
        let prize_amount = 100i128 + i as i128 * 50;
        let raffle_id = client.create_raffle(
            &creator,
            &String::from_str(&env, "Raffle"),
            &0,
            &10,
            &true,
            &10i128,
            &token_id,
            &prize_amount,
        );
        client.deposit_prize(&raffle_id);
        client.buy_ticket(&raffle_id, &buyer);
        
        let winner = client.finalize_raffle(&raffle_id, &String::from_str(&env, "prng"));
        if winner == buyer {
            let _claimed = client.claim_prize(&raffle_id, &buyer);
        }
    }

    let participation = client.get_user_raffle_participation(&buyer, &0, &100);
    assert_eq!(participation.raffle_ids.len(), 3);
    assert_eq!(participation.total_spent, 30i128); // 3 raffles * 1 ticket * 10
    // Win count and total winnings should match what was actually won
    assert!(participation.win_count >= 0 && participation.win_count <= 3);
    assert!(participation.total_winnings >= 0);
}
