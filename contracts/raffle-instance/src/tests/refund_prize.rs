//! Security tests for `refund_prize` (#755).
//!
//! These tests verify:
//! 1. When `prize_token != payment_token`, `refund_prize` returns prize tokens
//!    to the creator — not payment tokens — so the ticket-revenue pool is
//!    never touched.
//! 2. After `refund_prize` runs, all outstanding ticket refunds can still be
//!    satisfied in full (invariant: ticket pool ≥ tickets_sold * ticket_price).
//! 3. The reentrancy guard is acquired and released correctly.

use super::*;
use soroban_sdk::testutils::Address as _;

// ── helpers ──────────────────────────────────────────────────────────────────

/// Deploy a raffle with `payment_token` for ticket sales and a *separate*
/// `prize_token` for the prize escrow, then force the raffle into the
/// `Cancelled` state.
///
/// Returns:
///   - The deployed contract client
///   - The contract address
///   - The creator address
///   - A buyer address (has bought `ticket_count` tickets)
///   - The payment token client (for balance queries)
///   - The prize token client (for balance queries)
struct CancelledRaffleSetup<'a> {
    client: RaffleInstanceClient<'a>,
    contract_id: Address,
    creator: Address,
    buyer: Address,
    payment_token_client: token::StellarAssetClient<'a>,
    prize_token_client: token::StellarAssetClient<'a>,
    prize_amount: i128,
    ticket_price: i128,
    ticket_count: u32,
}

fn setup_cancelled_raffle_distinct_tokens(
    env: &Env,
    ticket_count: u32,
) -> CancelledRaffleSetup<'_> {
    let contract_id = env.register(RaffleInstance, ());
    let client = RaffleInstanceClient::new(env, &contract_id);

    // Use a plain Address for the factory so we can remove it from storage
    // and skip factory-specific call paths in buy_tickets.
    let factory = Address::generate(env);
    let admin = Address::generate(env);
    let creator = Address::generate(env);
    let buyer = Address::generate(env);

    // Two completely independent token contracts.
    let pay_admin = Address::generate(env);
    let prize_admin = Address::generate(env);

    let payment_token_addr = env
        .register_stellar_asset_contract_v2(pay_admin.clone())
        .address();
    let prize_token_addr = env
        .register_stellar_asset_contract_v2(prize_admin.clone())
        .address();

    let payment_token_client = token::StellarAssetClient::new(env, &payment_token_addr);
    let prize_token_client = token::StellarAssetClient::new(env, &prize_token_addr);

    let prize_amount = MIN_TICKET_PRICE * 50;
    let ticket_price = MIN_TICKET_PRICE;

    // Fund creator with prize tokens and payment tokens (for deposit_prize which
    // uses payment_token in the default implementation).
    payment_token_client.mint(&creator, &(prize_amount + ticket_price * ticket_count as i128));
    prize_token_client.mint(&creator, &prize_amount);
    // Fund buyer for ticket purchases.
    payment_token_client.mint(&buyer, &(ticket_price * ticket_count as i128 + 1_000_000));

    let config = RaffleConfig {
        description: String::from_str(env, "distinct-token refund test"),
        end_time: 0,
        no_deadline: true,
        max_tickets: ticket_count.max(1),
        max_tickets_per_tx: ticket_count.max(1),
        max_tickets_per_address: 0, // unlimited
        min_tickets: 1,
        allow_multiple: true,
        ticket_price,
        payment_token: payment_token_addr.clone(),
        prize_amount,
        prizes: soroban_sdk::vec![env, 10_000u32],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(env, &[0xABu8; 32]),
        claim_lockup_seconds: Some(0),
        swap_deadline_seconds: Some(0),
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
        unique_winners: false,
        bundles: soroban_sdk::Vec::new(env),
        // prize_token: None means the initializer defaults to payment_token.
        // We will overwrite it below via env.as_contract.
        prize_token: None,
        nft_contract: None,
    };

    client.init(&factory, &admin, &creator, &config);

    // Remove Factory key so that buy_tickets doesn't try to cross-contract-call
    // the factory for volume tracking.
    env.as_contract(&contract_id, || {
        env.storage().instance().remove(&DataKey::Factory);
    });

    // Overwrite prize_token in the Raffle struct to the *separate* token.
    // This simulates the future path where init properly wires prize_token from
    // the config, and is also the only way to exercise distinct-token paths
    // in tests until init is updated to honour config.prize_token.
    env.as_contract(&contract_id, || {
        let mut raffle = crate::read_raffle(env).expect("raffle must be initialized");
        raffle.prize_token = prize_token_addr.clone();
        crate::write_raffle(env, &raffle);
    });

    // deposit_prize pulls from payment_token by default (init.rs).  Since we
    // changed prize_token to a different contract AFTER the raffle was stored,
    // we simulate the prize deposit by directly minting prize_token into the
    // contract and flipping prize_deposited manually — this mirrors what a
    // corrected deposit_prize would do once init wires prize_token correctly.
    prize_token_client.mint(&contract_id, &prize_amount);
    env.as_contract(&contract_id, || {
        let mut raffle = crate::read_raffle(env).expect("raffle must be initialized");
        raffle.prize_deposited = true;
        raffle.status = RaffleStatus::Active;
        crate::write_raffle(env, &raffle);
    });

    // Buyer purchases tickets; these are paid in payment_token.
    if ticket_count > 0 {
        client.buy_tickets(&buyer, &ticket_count);
    }

    // Cancel the raffle (creator or admin cancel — both are valid).
    client.cancel_raffle(&CancelReason::CreatorCancelled);

    CancelledRaffleSetup {
        client,
        contract_id,
        creator,
        buyer,
        payment_token_client,
        prize_token_client,
        prize_amount,
        ticket_price,
        ticket_count,
    }
}

// ── test 1: refund_prize returns prize_token, not payment_token ──────────────

/// When prize_token differs from payment_token, `refund_prize` must transfer
/// from prize_token to the creator.  The payment_token balance in the contract
/// must remain unchanged (it belongs to ticket buyers).
#[test]
fn refund_prize_uses_prize_token_not_payment_token() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let setup = setup_cancelled_raffle_distinct_tokens(&env, 3);

    // Record contract balances before refund_prize.
    let contract_pay_before = soroban_sdk::token::Client::new(
        &env,
        &setup.payment_token_client.address,
    )
    .balance(&setup.contract_id);

    let contract_prize_before = soroban_sdk::token::Client::new(
        &env,
        &setup.prize_token_client.address,
    )
    .balance(&setup.contract_id);

    let creator_prize_before = soroban_sdk::token::Client::new(
        &env,
        &setup.prize_token_client.address,
    )
    .balance(&setup.creator);

    // refund_prize should succeed.
    setup.client.refund_prize();

    let contract_pay_after = soroban_sdk::token::Client::new(
        &env,
        &setup.payment_token_client.address,
    )
    .balance(&setup.contract_id);

    let contract_prize_after = soroban_sdk::token::Client::new(
        &env,
        &setup.prize_token_client.address,
    )
    .balance(&setup.contract_id);

    let creator_prize_after = soroban_sdk::token::Client::new(
        &env,
        &setup.prize_token_client.address,
    )
    .balance(&setup.creator);

    // The payment_token pool must be completely untouched.
    assert_eq!(
        contract_pay_after, contract_pay_before,
        "refund_prize must not touch the payment_token pool"
    );

    // The prize_token balance of the contract must decrease by prize_amount.
    assert_eq!(
        contract_prize_after,
        contract_prize_before - setup.prize_amount,
        "contract prize_token balance must decrease by prize_amount"
    );

    // The creator must receive exactly prize_amount in prize_token.
    assert_eq!(
        creator_prize_after,
        creator_prize_before + setup.prize_amount,
        "creator must receive prize_amount in prize_token"
    );
}

// ── test 2: ticket pool intact after refund_prize ────────────────────────────

/// After `refund_prize` succeeds, every outstanding ticket refund must still
/// be satisfiable in full.  Specifically, the contract's payment_token balance
/// must be ≥ tickets_sold * ticket_price.
#[test]
fn ticket_pool_intact_after_refund_prize() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let ticket_count: u32 = 5;
    let setup = setup_cancelled_raffle_distinct_tokens(&env, ticket_count);

    // Call refund_prize first.
    setup.client.refund_prize();

    // Now check the payment_token pool covers all outstanding ticket refunds.
    let contract_pay_balance = soroban_sdk::token::Client::new(
        &env,
        &setup.payment_token_client.address,
    )
    .balance(&setup.contract_id);

    let expected_minimum = setup.ticket_price * ticket_count as i128;

    assert!(
        contract_pay_balance >= expected_minimum,
        "payment_token pool ({contract_pay_balance}) must be ≥ tickets_sold * ticket_price \
         ({expected_minimum}) so that all ticket refunds can be satisfied"
    );
}

// ── test 3: all individual ticket refunds succeed after refund_prize ─────────

/// Extends the invariant test: actually execute every ticket refund and assert
/// they all succeed without a token-transfer failure.
#[test]
fn all_ticket_refunds_succeed_after_refund_prize_with_distinct_tokens() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let ticket_count: u32 = 4;
    let setup = setup_cancelled_raffle_distinct_tokens(&env, ticket_count);

    // refund_prize returns prize tokens to the creator.
    setup.client.refund_prize();

    // Every ticket (IDs 1..=ticket_count) must be refundable.
    for ticket_id in 1..=ticket_count {
        let refunded = setup.client.refund_ticket(&ticket_id);
        assert_eq!(
            refunded, setup.ticket_price,
            "ticket {ticket_id} refund amount must equal ticket_price"
        );
    }

    // The second call on ticket 1 must be rejected (already refunded).
    assert_eq!(
        setup.client.try_refund_ticket(&1),
        Err(Ok(Error::PrizeAlreadyClaimed)),
        "double-refund on ticket 1 must be rejected"
    );
}

// ── test 4: reentrancy guard is acquired and released ────────────────────────

/// refund_prize must hold the reentrancy guard while executing.  After a
/// successful call the guard must be released so subsequent calls (e.g.
/// refund_ticket) can acquire it.
#[test]
fn refund_prize_guard_is_released_after_success() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let ticket_count: u32 = 2;
    let setup = setup_cancelled_raffle_distinct_tokens(&env, ticket_count);

    // First call succeeds and releases the guard.
    setup.client.refund_prize();

    // Guard must not remain set in storage.
    let guard_still_set: bool = env.as_contract(&setup.contract_id, || {
        env.storage()
            .instance()
            .get(&DataKey::ReentrancyGuard)
            .unwrap_or(false)
    });
    assert!(
        !guard_still_set,
        "reentrancy guard must be released after refund_prize completes"
    );

    // refund_ticket on ticket 1 must also succeed — confirming the guard is
    // not stuck.
    let refunded = setup.client.refund_ticket(&1);
    assert_eq!(refunded, setup.ticket_price);
}

// ── test 5: double refund_prize is rejected ───────────────────────────────────

/// Calling refund_prize twice must fail on the second call because
/// `prize_deposited` is flipped to false on the first call.
#[test]
fn refund_prize_double_call_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let setup = setup_cancelled_raffle_distinct_tokens(&env, 1);

    setup.client.refund_prize();

    assert_eq!(
        setup.client.try_refund_prize(),
        Err(Ok(Error::PrizeNotDeposited)),
        "second refund_prize call must be rejected with PrizeNotDeposited"
    );
}
