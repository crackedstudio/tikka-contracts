#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::Address as _,
    token, Address, Env, String, Vec,
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
fn test_get_all_raffle_ids_pagination_oldest_first() {
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

    let id0 = create_raffle(&env, &client, &creator, &token_id, 1000);
    let id1 = create_raffle(&env, &client, &creator, &token_id, 1000);
    let id2 = create_raffle(&env, &client, &creator, &token_id, 0);
    let id3 = create_raffle(&env, &client, &creator, &token_id, 0);
    let id4 = create_raffle(&env, &client, &creator, &token_id, 1000);

    client.buy_ticket(&id2, &buyer);
    client.buy_ticket(&id3, &buyer);
    client.finalize_raffle(&id2);
    client.finalize_raffle(&id3);

    let all_ids = client.get_all_raffle_ids(&0u32, &10u32, &false);
    let mut expected = Vec::new(&env);
    expected.push_back(id0);
    expected.push_back(id1);
    expected.push_back(id2);
    expected.push_back(id3);
    expected.push_back(id4);
    assert_eq!(all_ids, expected);

    let page = client.get_all_raffle_ids(&1u32, &2u32, &false);
    let mut expected_page = Vec::new(&env);
    expected_page.push_back(id1);
    expected_page.push_back(id2);
    assert_eq!(page, expected_page);
}

#[test]
fn test_get_all_raffle_ids_pagination_newest_first() {
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

    let id0 = create_raffle(&env, &client, &creator, &token_id, 1000);
    let id1 = create_raffle(&env, &client, &creator, &token_id, 1000);
    let id2 = create_raffle(&env, &client, &creator, &token_id, 1000);
    let id3 = create_raffle(&env, &client, &creator, &token_id, 1000);
    let id4 = create_raffle(&env, &client, &creator, &token_id, 1000);

    let page = client.get_all_raffle_ids(&0u32, &3u32, &true);
    let mut expected_page = Vec::new(&env);
    expected_page.push_back(id4);
    expected_page.push_back(id3);
    expected_page.push_back(id2);
    assert_eq!(page, expected_page);

    let page_offset = client.get_all_raffle_ids(&2u32, &2u32, &true);
    let mut expected_offset = Vec::new(&env);
    expected_offset.push_back(id2);
    expected_offset.push_back(id1);
    assert_eq!(page_offset, expected_offset);
}

#[test]
fn test_get_all_raffle_ids_limit_capped() {
    let env = Env::default();
    env.mock_all_auths();

    let creator = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_id = token_contract.address();

    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);

    for _ in 0..120u32 {
        create_raffle(&env, &client, &creator, &token_id, 1000);
    }

    let ids = client.get_all_raffle_ids(&0u32, &250u32, &false);
    assert_eq!(ids.len(), 100);
}
