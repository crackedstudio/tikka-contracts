#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::Address as _,
    token, Address, Env, String,
};

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
fn test_buy_tickets_single() {
    let env = Env::default();
    env.mock_all_auths();

    let creator = Address::generate(&env);
    let buyer = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_id = token_contract.address();
    let token_client = token::Client::new(&env, &token_id);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_id);

    token_admin_client.mint(&buyer, &1_000);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let raffle_id = client.create_raffle(
        &creator,
        &String::from_str(&env, "Test Raffle"),
        &1000u64,
        &100u32,
        &true,
        &10i128,
        &token_id,
        &100i128,
    );

    let initial_balance = token_client.balance(&buyer);
    let tickets_sold = client.buy_tickets(&raffle_id, &buyer, &1u32);
    let final_balance = token_client.balance(&buyer);
    let raffle = client.get_raffle(&raffle_id);

    assert_eq!(tickets_sold, 1);
    assert_eq!(raffle.tickets_sold, 1);
    assert_eq!(initial_balance - final_balance, 10); // 1 ticket × 10 price
}

#[test]
fn test_buy_tickets_multiple() {
    let env = Env::default();
    env.mock_all_auths();

    let creator = Address::generate(&env);
    let buyer = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_id = token_contract.address();
    let token_client = token::Client::new(&env, &token_id);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_id);

    token_admin_client.mint(&buyer, &10_000);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let raffle_id = client.create_raffle(
        &creator,
        &String::from_str(&env, "Test Raffle"),
        &1000u64,
        &100u32,
        &true,
        &10i128,
        &token_id,
        &100i128,
    );

    let quantity = 5u32;
    let initial_balance = token_client.balance(&buyer);
    let tickets_sold = client.buy_tickets(&raffle_id, &buyer, &quantity);
    let final_balance = token_client.balance(&buyer);
    let raffle = client.get_raffle(&raffle_id);
    let tickets = client.get_tickets(&raffle_id);

    assert_eq!(tickets_sold, quantity);
    assert_eq!(raffle.tickets_sold, quantity);
    assert_eq!(initial_balance - final_balance, (quantity as i128) * 10); // 5 tickets × 10 price = 50
    assert_eq!(tickets.len(), quantity);
}

#[test]
fn test_buy_tickets_large_quantity() {
    let env = Env::default();
    env.mock_all_auths();

    let creator = Address::generate(&env);
    let buyer = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_id = token_contract.address();
    let token_client = token::Client::new(&env, &token_id);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_id);

    token_admin_client.mint(&buyer, &100_000);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let raffle_id = client.create_raffle(
        &creator,
        &String::from_str(&env, "Test Raffle"),
        &1000u64,
        &100u32,
        &true,
        &10i128,
        &token_id,
        &100i128,
    );

    let quantity = 100u32;
    let initial_balance = token_client.balance(&buyer);
    let tickets_sold = client.buy_tickets(&raffle_id, &buyer, &quantity);
    let final_balance = token_client.balance(&buyer);
    let raffle = client.get_raffle(&raffle_id);

    assert_eq!(tickets_sold, quantity);
    assert_eq!(raffle.tickets_sold, quantity);
    assert_eq!(initial_balance - final_balance, (quantity as i128) * 10); // 100 tickets × 10 price = 1000
}

#[test]
#[should_panic(expected = "multiple_tickets_not_allowed")]
fn test_buy_tickets_allow_multiple_false_rejects_multiple() {
    let env = Env::default();
    env.mock_all_auths();

    let creator = Address::generate(&env);
    let buyer = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_id = token_contract.address();
    let token_admin_client = token::StellarAssetClient::new(&env, &token_id);

    token_admin_client.mint(&buyer, &1_000);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let raffle_id = client.create_raffle(
        &creator,
        &String::from_str(&env, "Test Raffle"),
        &1000u64,
        &100u32,
        &false, // allow_multiple = false
        &10i128,
        &token_id,
        &100i128,
    );

    // Should panic because allow_multiple is false and quantity > 1
    client.buy_tickets(&raffle_id, &buyer, &5u32);
}

#[test]
#[should_panic(expected = "insufficient_tickets_available")]
fn test_buy_tickets_exceeds_max() {
    let env = Env::default();
    env.mock_all_auths();

    let creator = Address::generate(&env);
    let buyer = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_id = token_contract.address();
    let token_admin_client = token::StellarAssetClient::new(&env, &token_id);

    token_admin_client.mint(&buyer, &10_000);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let raffle_id = client.create_raffle(
        &creator,
        &String::from_str(&env, "Test Raffle"),
        &1000u64,
        &10u32, // max_tickets = 10
        &true,
        &10i128,
        &token_id,
        &100i128,
    );

    // Should panic because quantity (15) exceeds max_tickets (10)
    client.buy_tickets(&raffle_id, &buyer, &15u32);
}

#[test]
#[should_panic(expected = "quantity_zero")]
fn test_buy_tickets_zero_quantity() {
    let env = Env::default();
    env.mock_all_auths();

    let creator = Address::generate(&env);
    let buyer = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_id = token_contract.address();
    let token_admin_client = token::StellarAssetClient::new(&env, &token_id);

    token_admin_client.mint(&buyer, &1_000);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let raffle_id = client.create_raffle(
        &creator,
        &String::from_str(&env, "Test Raffle"),
        &1000u64,
        &10u32,
        &true,
        &10i128,
        &token_id,
        &100i128,
    );

    // Should panic because quantity is zero
    client.buy_tickets(&raffle_id, &buyer, &0u32);
}

#[test]
fn test_buy_tickets_allow_multiple_true_allows_multiple() {
    let env = Env::default();
    env.mock_all_auths();

    let creator = Address::generate(&env);
    let buyer = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_id = token_contract.address();
    let token_client = token::Client::new(&env, &token_id);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_id);

    token_admin_client.mint(&buyer, &10_000);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let raffle_id = client.create_raffle(
        &creator,
        &String::from_str(&env, "Test Raffle"),
        &1000u64,
        &100u32,
        &true, // allow_multiple = true
        &10i128,
        &token_id,
        &100i128,
    );

    // First purchase
    let tickets_sold_1 = client.buy_tickets(&raffle_id, &buyer, &3u32);
    assert_eq!(tickets_sold_1, 3);

    // Second purchase from same buyer should work
    let tickets_sold_2 = client.buy_tickets(&raffle_id, &buyer, &2u32);
    assert_eq!(tickets_sold_2, 5);

    let raffle = client.get_raffle(&raffle_id);
    assert_eq!(raffle.tickets_sold, 5);
    
    let initial_balance = token_client.balance(&buyer);
    assert_eq!(initial_balance, 10_000 - (5 * 10)); // 5 tickets × 10 price = 50
}
#[test]
fn test_get_active_raffle_ids_returns_active_raffles() {
    let env = Env::default();
    env.mock_all_auths();

    let creator = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_id = token_contract.address();

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let raffle_id_1 = client.create_raffle(
        &creator,
        &String::from_str(&env, "Raffle 1"),
        &10000u64,
        &10u32,
        &true,
        &10i128,
        &token_id,
        &100i128,
    );

    let raffle_id_2 = client.create_raffle(
        &creator,
        &String::from_str(&env, "Raffle 2"),
        &10000u64,
        &10u32,
        &true,
        &10i128,
        &token_id,
        &100i128,
    );

    let active_ids = client.get_active_raffle_ids(&0u32, &10u32);
    assert_eq!(active_ids.len(), 2);
    assert_eq!(active_ids.get(0).unwrap(), raffle_id_1);
    assert_eq!(active_ids.get(1).unwrap(), raffle_id_2);
}

#[test]
fn test_get_active_raffle_ids_excludes_finalized_raffles() {
    let env = Env::default();
    env.mock_all_auths();

    let creator = Address::generate(&env);
    let buyer = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_id = token_contract.address();
    let token_admin_client = token::StellarAssetClient::new(&env, &token_id);

    token_admin_client.mint(&buyer, &1_000);

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let raffle_id_1 = client.create_raffle(
        &creator,
        &String::from_str(&env, "Raffle 1"),
        &0u64,
        &10u32,
        &true,
        &10i128,
        &token_id,
        &100i128,
    );

    let raffle_id_2 = client.create_raffle(
        &creator,
        &String::from_str(&env, "Raffle 2"),
        &10000u64,
        &10u32,
        &true,
        &10i128,
        &token_id,
        &100i128,
    );

    client.buy_ticket(&raffle_id_1, &buyer);
    client.finalize_raffle(&raffle_id_1);

    let active_ids = client.get_active_raffle_ids(&0u32, &10u32);
    assert_eq!(active_ids.len(), 1);
    assert_eq!(active_ids.get(0).unwrap(), raffle_id_2);
}

#[test]
fn test_get_active_raffle_ids_filters_by_end_time() {
    let env = Env::default();
    env.mock_all_auths();

    let creator = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_id = token_contract.address();

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    client.create_raffle(
        &creator,
        &String::from_str(&env, "Expired Raffle"),
        &0u64,
        &10u32,
        &true,
        &10i128,
        &token_id,
        &100i128,
    );

    let raffle_id_2 = client.create_raffle(
        &creator,
        &String::from_str(&env, "Active Raffle"),
        &10000u64,
        &10u32,
        &true,
        &10i128,
        &token_id,
        &100i128,
    );

    let active_ids = client.get_active_raffle_ids(&0u32, &10u32);
    assert_eq!(active_ids.len(), 1);
    assert_eq!(active_ids.get(0).unwrap(), raffle_id_2);
}

#[test]
fn test_get_active_raffle_ids_pagination_offset() {
    let env = Env::default();
    env.mock_all_auths();

    let creator = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_id = token_contract.address();

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let mut raffle_ids = Vec::new(&env);
    for _ in 0..5 {
        let raffle_id = client.create_raffle(
            &creator,
            &String::from_str(&env, "Raffle"),
            &10000u64,
            &10u32,
            &true,
            &10i128,
            &token_id,
            &100i128,
        );
        raffle_ids.push_back(raffle_id);
    }

    let page1 = client.get_active_raffle_ids(&0u32, &2u32);
    assert_eq!(page1.len(), 2);
    assert_eq!(page1.get(0).unwrap(), raffle_ids.get(0).unwrap());
    assert_eq!(page1.get(1).unwrap(), raffle_ids.get(1).unwrap());

    let page2 = client.get_active_raffle_ids(&2u32, &2u32);
    assert_eq!(page2.len(), 2);
    assert_eq!(page2.get(0).unwrap(), raffle_ids.get(2).unwrap());
    assert_eq!(page2.get(1).unwrap(), raffle_ids.get(3).unwrap());

    let page3 = client.get_active_raffle_ids(&4u32, &2u32);
    assert_eq!(page3.len(), 1);
    assert_eq!(page3.get(0).unwrap(), raffle_ids.get(4).unwrap());
}

#[test]
fn test_get_active_raffle_ids_pagination_limit() {
    let env = Env::default();
    env.mock_all_auths();

    let creator = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_id = token_contract.address();

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    for _ in 0..10 {
        client.create_raffle(
            &creator,
            &String::from_str(&env, "Raffle"),
            &10000u64,
            &10u32,
            &true,
            &10i128,
            &token_id,
            &100i128,
        );
    }

    let result = client.get_active_raffle_ids(&0u32, &5u32);
    assert_eq!(result.len(), 5);
}

#[test]
fn test_get_active_raffle_ids_limit_max_100() {
    let env = Env::default();
    env.mock_all_auths();

    let creator = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_id = token_contract.address();

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    for _ in 0..10 {
        client.create_raffle(
            &creator,
            &String::from_str(&env, "Raffle"),
            &10000u64,
            &10u32,
            &true,
            &10i128,
            &token_id,
            &100i128,
        );
    }

    let result = client.get_active_raffle_ids(&0u32, &200u32);
    assert_eq!(result.len(), 10);
}

#[test]
fn test_get_active_raffle_ids_empty_result() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let result = client.get_active_raffle_ids(&0u32, &10u32);
    assert_eq!(result.len(), 0);
}

#[test]
fn test_get_active_raffle_ids_with_zero_raffles() {
    let env = Env::default();

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let result = client.get_active_raffle_ids(&0u32, &10u32);
    assert_eq!(result.len(), 0);
}

#[test]
fn test_get_active_raffle_ids_with_one_raffle() {
    let env = Env::default();
    env.mock_all_auths();

    let creator = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_id = token_contract.address();

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    let raffle_id = client.create_raffle(
        &creator,
        &String::from_str(&env, "Single Raffle"),
        &10000u64,
        &10u32,
        &true,
        &10i128,
        &token_id,
        &100i128,
    );

    let result = client.get_active_raffle_ids(&0u32, &10u32);
    assert_eq!(result.len(), 1);
    assert_eq!(result.get(0).unwrap(), raffle_id);
}

#[test]
fn test_get_active_raffle_ids_offset_beyond_available() {
    let env = Env::default();
    env.mock_all_auths();

    let creator = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_id = token_contract.address();

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    for _ in 0..3 {
        client.create_raffle(
            &creator,
            &String::from_str(&env, "Raffle"),
            &10000u64,
            &10u32,
            &true,
            &10i128,
            &token_id,
            &100i128,
        );
    }

    let result = client.get_active_raffle_ids(&10u32, &5u32);
    assert_eq!(result.len(), 0);
}