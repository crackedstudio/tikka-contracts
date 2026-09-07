use raffle_shared::{RaffleConfig, RandomnessSource};
use soroban_sdk::{
    testutils::Address as _, token::StellarAssetClient, Address, BytesN, Env, String, Vec,
};
use tikka_raffle_instance::{Contract, ContractClient, DataKey, RaffleStatus};

/// Minimal factory stub. The raffle contract's `setup` removes its `Factory`
/// key, which makes `require_global_not_paused` fail and every `buy_tickets`
/// call error. `refund_cancel` re-registers this live stub so the fuzzer can
/// actually reach the refund paths under test (#827).
#[soroban_sdk::contract]
struct MockFactory;

#[soroban_sdk::contractimpl]
impl MockFactory {
    pub fn is_global_paused(_env: soroban_sdk::Env) -> bool {
        false
    }

    pub fn record_volume(_env: soroban_sdk::Env, _token: Address, _amount: i128) {}

    pub fn track_participant(_env: soroban_sdk::Env, _participant: Address) {}

    pub fn record_leaderboard_entry(
        _env: soroban_sdk::Env,
        _raffle_id: Address,
        _tickets: i128,
        _prize_amount: i128,
        _volume: i128,
    ) {
    }
}

pub fn setup(
    env: &Env,
    max_tickets: u32,
) -> (ContractClient<'_>, Address, Address, StellarAssetClient<'_>) {
    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(env, &contract_id);
    let factory = Address::generate(env);
    let admin = Address::generate(env);
    let creator = Address::generate(env);
    let token_admin = Address::generate(env);
    let payment_token = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token = StellarAssetClient::new(env, &payment_token);
    token.mint(&creator, &(max_tickets as i128 * 20_000));

    let config = RaffleConfig {
        description: String::from_str(env, "fuzz"),
        end_time: 0,
        no_deadline: true,
        max_tickets,
        max_tickets_per_tx: max_tickets,
        max_tickets_per_address: 0,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: 10_000,
        payment_token,
        prize_amount: max_tickets as i128 * 10_000,
        prizes: Vec::from_array(env, [10_000]),
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(env, &[1; 32]),
        claim_lockup_seconds: Some(0),
        swap_deadline_seconds: Some(0),
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
        env.storage()
            .instance()
            .remove(&tikka_raffle_instance::DataKey::Factory);
    });
    client.deposit_prize();
    assert_solvent(env, &client);
    (client, creator, admin, token)
}

pub fn buy(data: &[u8]) {
    let env = Env::default();
    env.mock_all_auths();
    let (client, creator, _admin, token) = setup(&env, 50);
    let buyer = Address::generate(&env);
    token.mint(&buyer, &1_000_000);
    let quantity = (data.first().copied().unwrap_or(1) as u32 % 5) + 1;
    let _ = client.try_buy_tickets(&buyer, &quantity);
    assert_solvent(&env, &client);
}

pub fn finalize(data: &[u8]) {
    let env = Env::default();
    env.mock_all_auths();
    let (client, creator, _admin, token) = setup(&env, 1);
    token.mint(&creator, &10_000);
    let _ = client.try_finalize_raffle();
    assert_solvent(&env, &client);
    let _ = data;
}

pub fn refund_cancel(data: &[u8]) {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _creator, _admin, payment_token) = setup(&env, 12);
    let token = payment_token;

    // Re-point the factory at a live stub so buys succeed (see `MockFactory`).
    let mock = env.register(MockFactory, ());
    env.as_contract(&client.address, || {
        env.storage().instance().set(&DataKey::Factory, &mock);
    });

    let contract_id = client.address.clone();
    let buyers: std::vec::Vec<Address> = (0..3).map(|_| Address::generate(&env)).collect();
    for b in &buyers {
        token.mint(b, &1_000_000);
    }

    for (i, &op) in data.iter().enumerate() {
        match op % 7 {
            0 | 1 => {
                let buyer = buyers[i % buyers.len()].clone();
                let qty = (i as u32 % 3) + 1;
                let _ = client.try_buy_tickets(&buyer, &qty);
            }
            2 => {
                let _ = client.try_cancel_raffle(&raffle_shared::CancelReason::CreatorCancelled);
            }
            3 => {
                let _ = client.try_cancel_raffle(&raffle_shared::CancelReason::AdminCancelled);
            }
            4 => {
                let _ = client.try_finalize_raffle();
            }
            5 => {
                let ticket_id = (i as u32 % 12) + 1;
                let _ = client.try_refund_ticket(&ticket_id);
            }
            _ => {
                let _ = client.try_refund_prize();
            }
        }

        // Solvency invariant after every operation (#827): the contract must
        // still hold enough to cover every un-refunded ticket and the prize.
        if let Ok(Ok(raffle)) = client.try_get_raffle() {
            let mut outstanding = 0i128;
            for id in 1..=raffle.tickets_sold {
                let refunded = env.as_contract(&contract_id, || {
                    env.storage().persistent().has(&DataKey::TicketRefunded(id))
                });
                if !refunded {
                    outstanding += raffle.ticket_price;
                }
            }
            if raffle.prize_deposited {
                outstanding += raffle.prize_amount;
            }
            let held = token.balance(&contract_id);
            assert!(
                held >= outstanding,
                "refund solvency violated (op {i}): contract holds {held} but owes {outstanding}"
            );
            assert!(raffle.tickets_sold <= raffle.max_tickets);
        }
    }
}

pub fn commit_reveal(data: &[u8]) {
    let env = Env::default();
    env.mock_all_auths();
    let (client, creator, _admin, token) = setup(&env, 2);
    let buyer = Address::generate(&env);
    token.mint(&buyer, &100_000);
    let _ = client.try_buy_tickets(&buyer, &1);
    assert_solvent(&env, &client);
    let hash = BytesN::from_array(&env, &[data.first().copied().unwrap_or(0); 32]);
    let _ = client.try_submit_commit(&1, &hash);
    assert_solvent(&env, &client);
}

pub fn lifecycle(data: &[u8]) {
    let env = Env::default();
    env.mock_all_auths();
    let (client, creator, admin, token) = setup(&env, 20);
    let buyer = Address::generate(&env);
    token.mint(&buyer, &1_000_000);
    let mut paused = false;

    for operation in data.iter().copied() {
        match operation % 6 {
            0 if !paused => {
                let quantity = (operation as u32 % 3) + 1;
                let _ = client.try_buy_tickets(&buyer, &quantity);
            }
            1 => {
                let _ = client.try_finalize_raffle();
            }
            2 => {
                let _ = client.try_claim_prize(&buyer, 0);
            }
            3 => {
                let ticket_id = (operation as u32 % 20) + 1;
                let _ = client.try_refund_ticket(&ticket_id);
            }
            4 => {
                let _ = client.try_cancel_raffle(&raffle_shared::CancelReason::AdminCancelled);
            }
            _ => {
                let result = if paused {
                    client.try_unpause()
                } else {
                    client.try_pause()
                };
                if result.is_ok() {
                    paused = !paused;
                }
            }
        }

        assert_solvent(&env, &client);

        if let Ok(Ok(raffle)) = client.try_get_raffle() {
            assert!(raffle.tickets_sold <= raffle.max_tickets);
            assert!(matches!(
                raffle.status,
                RaffleStatus::PendingPrize
                    | RaffleStatus::Active
                    | RaffleStatus::Drawing
                    | RaffleStatus::Finalized
                    | RaffleStatus::Claimed
                    | RaffleStatus::Cancelled
                    | RaffleStatus::Failed
            ));
            assert!(token.balance(&client.address) >= 0);
        }
    }
}
