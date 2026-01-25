#![cfg(test)]

use super::*;
use soroban_sdk::{
    TryIntoVal,
    testutils::{Address as _, Events},
    token, Address, Env, String,
};

fn create_raffle(
    env: &Env,
    client: &ContractClient,
    creator: &Address,
    token_id: &Address,
    end_time: u64,
) -> u64 {
    client.create_raffle(
        creator,
        &String::from_str(env, "Test Raffle"),
        &end_time,
        &10u32,
        &true,
        &1i128,
        token_id,
        &10i128,
    )
}

#[test]
fn test_basic_raffle_flow() {
    let env = Env::default();
    env.mock_all_auths();

    let creator = Address::generate(&env);
    let buyer = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_id = token_contract.address();
    let token_client = token::Client::new(&env, &token_id);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_id);

    token_admin_client.mint(&creator, &1_000);
    token_admin_client.mint(&buyer, &1_000);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let raffle_id = client.create_raffle(
        &creator,
        &String::from_str(&env, "Demo Raffle"),
        &0u64,
        &10u32,
        &false,
        &10i128,
        &token_id,
        &100i128,
    );

    client.deposit_prize(&raffle_id);
    client.buy_ticket(&raffle_id, &buyer);
    let winner = client.finalize_raffle(&raffle_id);
    let claimed_amount = client.claim_prize(&raffle_id, &winner);

    let winner_balance = token_client.balance(&winner);
    let creator_balance = token_client.balance(&creator);

    assert_eq!(claimed_amount, 100i128);
    assert_eq!(winner_balance, 1_090);
    assert_eq!(creator_balance, 900);
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
