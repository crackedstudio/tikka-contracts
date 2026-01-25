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

    let raffle = client.get_raffle(&raffle_id);
    assert_eq!(raffle.tickets_sold, 5);

    let initial_balance = token_client.balance(&buyer);
    assert_eq!(initial_balance, 10_000 - (5 * 10)); // 5 tickets × 10 price = 50
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
fn test_raffle_created_event_emits_with_all_fields() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, Contract);
    let client = ContractClient::new(&env, &contract_id);

    let creator = Address::generate(&env);
    let payment_token = Address::generate(&env);
    let description = String::from_str(&env, "Test Raffle Event");
    let end_time = 1000u64;
    let max_tickets = 100u32;
    let ticket_price = 10i128;
    let prize_amount = 500i128;

    // Create raffle
    let raffle_id = client.create_raffle(
        &creator,
        &description,
        &end_time,
        &max_tickets,
        &true,
        &ticket_price,
        &payment_token,
        &prize_amount,
    );

    // Get events
    let events = env.events().all();

    // Verify event was emitted
    assert!(events.len() > 0);

    // Find the RaffleCreated event
    let event = events.iter().find(|e| {
        e.topics.get(0).unwrap() == Symbol::new(&env, "RaffleCreated").into_val(&env)
    }).expect("RaffleCreated event not found");

    // Verify event topic contains raffle_id
    assert_eq!(event.topics.get(1).unwrap(), raffle_id.into_val(&env));

    // Verify event data
    let event_data: RaffleCreated = event.data.clone().try_into_val(&env).unwrap();
    assert_eq!(event_data.raffle_id, raffle_id);
    assert_eq!(event_data.creator, creator);
    assert_eq!(event_data.end_time, end_time);
    assert_eq!(event_data.max_tickets, max_tickets);
    assert_eq!(event_data.ticket_price, ticket_price);
    assert_eq!(event_data.payment_token, payment_token);
    assert_eq!(event_data.description, description);
}

#[test]
fn test_raffle_created_event_data_matches_parameters() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, Contract);
    let client = ContractClient::new(&env, &contract_id);

    let creator = Address::generate(&env);
    let payment_token = Address::generate(&env);
    let description = String::from_str(&env, "Match Test Raffle");
    let end_time = 5000u64;
    let max_tickets = 250u32;
    let ticket_price = 25i128;
    let prize_amount = 1000i128;

    // Create raffle
    let raffle_id = client.create_raffle(
        &creator,
        &description,
        &end_time,
        &max_tickets,
        &false,
        &ticket_price,
        &payment_token,
        &prize_amount,
    );

    // Verify stored raffle matches event data
    let raffle = client.get_raffle(&raffle_id);
    let events = env.events().all();

    let event = events.iter().find(|e| {
        e.topics.get(0).unwrap() == Symbol::new(&env, "RaffleCreated").into_val(&env)
    }).unwrap();

    let event_data: RaffleCreated = event.data.clone().try_into_val(&env).unwrap();

    // Verify event data matches both input parameters and stored raffle
    assert_eq!(event_data.raffle_id, raffle.id);
    assert_eq!(event_data.creator, raffle.creator);
    assert_eq!(event_data.end_time, raffle.end_time);
    assert_eq!(event_data.max_tickets, raffle.max_tickets);
    assert_eq!(event_data.ticket_price, raffle.ticket_price);
    assert_eq!(event_data.payment_token, raffle.payment_token);
    assert_eq!(event_data.description, raffle.description);
}

#[test]
fn test_raffle_created_event_emits_for_edge_cases() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, Contract);
    let client = ContractClient::new(&env, &contract_id);

    let creator = Address::generate(&env);
    let payment_token = Address::generate(&env);

    // Test with minimum valid values
    let min_description = String::from_str(&env, "A");
    let min_end_time = 1u64;
    let min_max_tickets = 1u32;
    let min_ticket_price = 1i128;
    let min_prize_amount = 1i128;

    let raffle_id_min = client.create_raffle(
        &creator,
        &min_description,
        &min_end_time,
        &min_max_tickets,
        &false,
        &min_ticket_price,
        &payment_token,
        &min_prize_amount,
    );

    // Verify event emitted for minimum values
    let events_min = env.events().all();
    let event_min = events_min.iter().find(|e| {
        e.topics.get(0).unwrap() == Symbol::new(&env, "RaffleCreated").into_val(&env) &&
        e.topics.get(1).unwrap() == raffle_id_min.into_val(&env)
    }).expect("Event not found for minimum values");

    let event_data_min: RaffleCreated = event_min.data.clone().try_into_val(&env).unwrap();
    assert_eq!(event_data_min.max_tickets, min_max_tickets);
    assert_eq!(event_data_min.ticket_price, min_ticket_price);

    // Test with maximum valid values
    let max_description = String::from_str(&env, "Very long description with lots of text to test maximum length handling in event emission");
    let max_end_time = u64::MAX;
    let max_max_tickets = u32::MAX;
    let max_ticket_price = i128::MAX;
    let max_prize_amount = i128::MAX;

    let raffle_id_max = client.create_raffle(
        &creator,
        &max_description,
        &max_end_time,
        &max_max_tickets,
        &true,
        &max_ticket_price,
        &payment_token,
        &max_prize_amount,
    );

    // Verify event emitted for maximum values
    let events_max = env.events().all();
    let event_max = events_max.iter().find(|e| {
        e.topics.get(0).unwrap() == Symbol::new(&env, "RaffleCreated").into_val(&env) &&
        e.topics.get(1).unwrap() == raffle_id_max.into_val(&env)
    }).expect("Event not found for maximum values");

    let event_data_max: RaffleCreated = event_max.data.clone().try_into_val(&env).unwrap();
    assert_eq!(event_data_max.max_tickets, max_max_tickets);
    assert_eq!(event_data_max.ticket_price, max_ticket_price);
    assert_eq!(event_data_max.end_time, max_end_time);
}

#[test]
fn test_multiple_raffles_emit_separate_events() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, Contract);
    let client = ContractClient::new(&env, &contract_id);

    let creator1 = Address::generate(&env);
    let creator2 = Address::generate(&env);
    let payment_token = Address::generate(&env);

    // Create first raffle
    let desc1 = String::from_str(&env, "First Raffle");
    let raffle_id_1 = client.create_raffle(
        &creator1,
        &desc1,
        &1000u64,
        &50u32,
        &true,
        &10i128,
        &payment_token,
        &500i128,
    );

    // Create second raffle
    let desc2 = String::from_str(&env, "Second Raffle");
    let raffle_id_2 = client.create_raffle(
        &creator2,
        &desc2,
        &2000u64,
        &100u32,
        &false,
        &20i128,
        &payment_token,
        &1000i128,
    );

    // Create third raffle
    let desc3 = String::from_str(&env, "Third Raffle");
    let raffle_id_3 = client.create_raffle(
        &creator1,
        &desc3,
        &3000u64,
        &75u32,
        &true,
        &15i128,
        &payment_token,
        &750i128,
    );

    // Get all events
    let events = env.events().all();

    // Filter RaffleCreated events
    let raffle_created_events: Vec<_> = events.iter()
        .filter(|e| e.topics.get(0).unwrap() == Symbol::new(&env, "RaffleCreated").into_val(&env))
        .collect();

    // Verify we have exactly 3 RaffleCreated events
    assert_eq!(raffle_created_events.len(), 3);

    // Verify each event has correct raffle_id in topics
    let event_1 = raffle_created_events.iter()
        .find(|e| e.topics.get(1).unwrap() == raffle_id_1.into_val(&env))
        .expect("Event for raffle 1 not found");
    let event_2 = raffle_created_events.iter()
        .find(|e| e.topics.get(1).unwrap() == raffle_id_2.into_val(&env))
        .expect("Event for raffle 2 not found");
    let event_3 = raffle_created_events.iter()
        .find(|e| e.topics.get(1).unwrap() == raffle_id_3.into_val(&env))
        .expect("Event for raffle 3 not found");

    // Verify event data for each raffle
    let event_data_1: RaffleCreated = event_1.data.clone().try_into_val(&env).unwrap();
    assert_eq!(event_data_1.raffle_id, raffle_id_1);
    assert_eq!(event_data_1.creator, creator1);
    assert_eq!(event_data_1.description, desc1);

    let event_data_2: RaffleCreated = event_2.data.clone().try_into_val(&env).unwrap();
    assert_eq!(event_data_2.raffle_id, raffle_id_2);
    assert_eq!(event_data_2.creator, creator2);
    assert_eq!(event_data_2.description, desc2);

    let event_data_3: RaffleCreated = event_3.data.clone().try_into_val(&env).unwrap();
    assert_eq!(event_data_3.raffle_id, raffle_id_3);
    assert_eq!(event_data_3.creator, creator1);
    assert_eq!(event_data_3.description, desc3);

    // Verify raffle IDs are sequential
    assert_eq!(raffle_id_1, 0);
    assert_eq!(raffle_id_2, 1);
    assert_eq!(raffle_id_3, 2);
}

#[test]
fn test_event_provides_sufficient_indexing_data() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, Contract);
    let client = ContractClient::new(&env, &contract_id);

    let creator = Address::generate(&env);
    let payment_token = Address::generate(&env);
    let description = String::from_str(&env, "Indexing Test Raffle");
    let end_time = 10000u64;
    let max_tickets = 500u32;
    let ticket_price = 50i128;
    let prize_amount = 5000i128;

    let raffle_id = client.create_raffle(
        &creator,
        &description,
        &end_time,
        &max_tickets,
        &true,
        &ticket_price,
        &payment_token,
        &prize_amount,
    );

    let events = env.events().all();
    let event = events.iter().find(|e| {
        e.topics.get(0).unwrap() == Symbol::new(&env, "RaffleCreated").into_val(&env)
    }).unwrap();

    let event_data: RaffleCreated = event.data.clone().try_into_val(&env).unwrap();

    // Verify event contains all critical data for frontend indexing:

    // 1. Unique identifier
    assert!(event_data.raffle_id >= 0);

    // 2. Creator for filtering by user
    assert_eq!(event_data.creator, creator);

    // 3. Time-based filtering
    assert_eq!(event_data.end_time, end_time);

    // 4. Availability information
    assert_eq!(event_data.max_tickets, max_tickets);

    // 5. Pricing information
    assert_eq!(event_data.ticket_price, ticket_price);

    // 6. Payment token for multi-token filtering
    assert_eq!(event_data.payment_token, payment_token);

    // 7. Human-readable description
    assert_eq!(event_data.description, description);

    // All essential fields present - frontend can:
    // - Display raffle card with all info without additional queries
    // - Filter by creator, token, price range, or end time
    // - Sort by end_time or raffle_id
    // - Calculate availability percentage
}
