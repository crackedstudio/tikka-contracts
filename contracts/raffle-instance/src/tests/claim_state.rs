use super::*;

fn claim_state_fixture(
    env: &Env,
) -> (RaffleInstanceClient<'_>, Address, Address, Address) {
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let factory = Address::generate(env);
    let admin = Address::generate(env);
    let creator = Address::generate(env);
    let buyer_a = Address::generate(env);
    let buyer_b = Address::generate(env);
    let token_admin = Address::generate(env);
    let token = env.register_stellar_asset_contract_v2(token_admin).address();
    StellarAssetClient::new(env, &token).mint(&creator, &1_000_000);
    StellarAssetClient::new(env, &token).mint(&buyer_a, &1_000_000);
    StellarAssetClient::new(env, &token).mint(&buyer_b, &1_000_000);

    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(env, &contract_id);
    let config = RaffleConfig {
        description: String::from_str(env, "claim state"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 2,
        max_tickets_per_tx: 1,
        max_tickets_per_address: 0,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token: token,
        prize_amount: MIN_TICKET_PRICE * 10,
        prizes: soroban_sdk::vec![env, 5_000, 5_000],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(env, &[91; 32]),
        claim_lockup_seconds: Some(0),
        claim_expiry_seconds: None,
        swap_deadline_seconds: Some(0),
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
        unique_winners: true,
        bundles: soroban_sdk::Vec::new(env),
        prize_token: None,
        nft_contract: None,
    };

    client.init(&factory, &admin, &creator, &config);
    client.deposit_prize();
    client.buy_tickets(&buyer_a, &1);
    client.buy_tickets(&buyer_b, &1);
    env.ledger().set_timestamp(2_000);
    client.finalize_raffle();
    env.ledger().set_timestamp(2_001);

    (client, buyer_a, buyer_b, contract_id)
}

#[test]
fn second_claim_for_same_tier_is_rejected() {
    let env = Env::default();
    let (client, _buyer_a, _buyer_b, _) = claim_state_fixture(&env);
    let winner = client.get_raffle().winners.get(0).unwrap().address;

    client.claim_prize(&winner, &0);
    assert_eq!(
        client.try_claim_prize(&winner, &0),
        Err(Ok(Error::PrizeAlreadyClaimed))
    );
}

#[test]
fn all_tiers_claimed_transitions_raffle_to_claimed() {
    let env = Env::default();
    let (client, _buyer_a, _buyer_b, _) = claim_state_fixture(&env);
    let raffle = client.get_raffle();

    for tier_index in 0..raffle.winners.len() {
        let winner = raffle.winners.get(tier_index).unwrap().address;
        client.claim_prize(&winner, &tier_index);
    }

    assert_eq!(client.get_raffle().status, RaffleStatus::Claimed);
}
