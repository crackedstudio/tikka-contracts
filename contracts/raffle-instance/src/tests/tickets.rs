use super::*;
use soroban_sdk::testutils::Address as _;

struct TicketSetup<'a> {
    client: ContractClient<'a>,
    contract_id: Address,
    admin: Address,
    creator: Address,
    buyer: Address,
    recipient: Address,
    token: token::StellarAssetClient<'a>,
}

fn setup(env: &Env, cap: u32, allow_multiple: bool, max_tickets: u32) -> TicketSetup<'_> {
    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(env, &contract_id);
    let factory = env.register(MockFactory, ());
    let admin = Address::generate(env);
    let creator = Address::generate(env);
    let buyer = Address::generate(env);
    let recipient = Address::generate(env);
    let token_admin = Address::generate(env);
    let (payment_token, token) = create_token(env, &token_admin);

    token.mint(&creator, &1_000_000_000);
    token.mint(&buyer, &1_000_000_000);
    let config = RaffleConfig {
        description: String::from_str(env, "ticket cap"),
        end_time: 0,
        no_deadline: true,
        max_tickets,
        max_tickets_per_tx: max_tickets,
        max_tickets_per_address: cap,
        min_tickets: 1,
        allow_multiple,
        ticket_price: MIN_TICKET_PRICE,
        payment_token,
        prize_amount: MIN_TICKET_PRICE * max_tickets as i128,
        prizes: vec![env, 10_000u32],
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
    env.as_contract(&contract_id, || env.storage().instance().remove(&DataKey::Factory));
    client.deposit_prize();

    TicketSetup {
        client,
        contract_id,
        admin,
        creator,
        buyer,
        recipient,
        token,
    }
}

#[test]
fn buying_exactly_the_cap_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let setup = setup(&env, 5, true, 10);

    assert_eq!(setup.client.buy_tickets(&setup.buyer, &5), 5);
    assert_eq!(setup.client.get_remaining_ticket_allowance(&setup.buyer), 0);
}

#[test]
fn buying_beyond_the_cap_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let setup = setup(&env, 5, true, 10);

    setup.client.buy_tickets(&setup.buyer, &5);
    assert_eq!(
        setup.client.try_buy_tickets(&setup.buyer, &1),
        Err(Ok(Error::ExceedsMaxTicketsPerAddress))
    );
}

#[test]
fn cap_is_enforced_across_transactions() {
    let env = Env::default();
    env.mock_all_auths();
    let setup = setup(&env, 5, true, 10);

    setup.client.buy_tickets(&setup.buyer, &3);
    assert_eq!(
        setup.client.try_buy_tickets(&setup.buyer, &3),
        Err(Ok(Error::ExceedsMaxTicketsPerAddress))
    );
    assert_eq!(setup.client.get_remaining_ticket_allowance(&setup.buyer), 2);
}

#[test]
fn zero_cap_is_unlimited_up_to_raffle_capacity() {
    let env = Env::default();
    env.mock_all_auths();
    let setup = setup(&env, 0, true, 10);

    setup.client.buy_tickets(&setup.buyer, &5);
    assert_eq!(setup.client.buy_tickets(&setup.buyer, &5), 10);
    assert_eq!(setup.client.get_remaining_ticket_allowance(&setup.buyer), 0);
}

#[test]
fn configured_cap_supersedes_allow_multiple() {
    let env = Env::default();
    env.mock_all_auths();
    let setup = setup(&env, 3, false, 10);

    assert_eq!(setup.client.buy_tickets(&setup.buyer, &2), 2);
    assert_eq!(setup.client.get_remaining_ticket_allowance(&setup.buyer), 1);
}

#[test]
fn gifted_tickets_count_against_recipient_cap() {
    let env = Env::default();
    env.mock_all_auths();
    let setup = setup(&env, 2, true, 10);

    setup.client.buy_tickets_for(&setup.buyer, &setup.recipient, &2);
    assert_eq!(setup.client.get_remaining_ticket_allowance(&setup.recipient), 0);
    assert_eq!(setup.client.get_remaining_ticket_allowance(&setup.buyer), 2);
    assert_eq!(
        setup.client.try_buy_tickets_for(&setup.buyer, &setup.recipient, &1),
        Err(Ok(Error::ExceedsMaxTicketsPerAddress))
    );
}

#[test]
fn cap_cannot_exceed_max_tickets() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(&env, &contract_id);
    let factory = env.register(MockFactory, ());
    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let (payment_token, _token) = create_token(&env, &token_admin);
    let config = RaffleConfig {
        description: String::from_str(&env, "invalid cap"),
        end_time: 0,
        no_deadline: true,
        max_tickets: 2,
        max_tickets_per_tx: 2,
        max_tickets_per_address: 3,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price: MIN_TICKET_PRICE,
        payment_token,
        prize_amount: MIN_TICKET_PRICE * 2,
        prizes: vec![&env, 10_000u32],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(&env, &[1; 32]),
        claim_lockup_seconds: Some(0),
        swap_deadline_seconds: Some(0),
        early_bird_ticket_percentage: 0,
        early_bird_discount_bp: 0,
        category: None,
        unique_winners: false,
        bundles: Vec::new(&env),
        prize_token: None,
        nft_contract: None,
    };

    assert_eq!(
        client.try_init(&factory, &admin, &creator, &config),
        Err(Ok(Error::InvalidParameters))
    );
}

// ============================================================================
// PRICING INVARIANT TESTS
// ============================================================================
// These tests lock down the documented guarantee that preview_buy and
// buy_tickets use identical pricing logic.
//
// The invariant:
//   For every valid quantity q where buy_tickets(q) succeeds:
//   preview_buy(q).total == balance_before - balance_after
//
// These tests currently document expected behavior. The underlying
// calculate_buy_quote helper and BuyQuote structure do not yet exist,
// and buy_tickets currently uses flat pricing (ticket_price * quantity)
// without early-bird logic.
// ============================================================================

fn setup_pricing_test(
    env: &Env,
    max_tickets: u32,
    ticket_price: i128,
    early_bird_percentage: u32,
    early_bird_discount_bp: u32,
) -> TicketSetup<'_> {
    let contract_id = env.register(Contract, ());
    let client = ContractClient::new(env, &contract_id);
    let factory = env.register(MockFactory, ());
    let admin = Address::generate(env);
    let creator = Address::generate(env);
    let buyer = Address::generate(env);
    let recipient = Address::generate(env);
    let token_admin = Address::generate(env);
    let (payment_token, token) = create_token(env, &token_admin);

    token.mint(&creator, &1_000_000_000_000);
    token.mint(&buyer, &1_000_000_000_000);

    let config = RaffleConfig {
        description: String::from_str(env, "pricing test"),
        end_time: 0,
        no_deadline: true,
        max_tickets,
        max_tickets_per_tx: max_tickets,
        max_tickets_per_address: 0,
        min_tickets: 1,
        allow_multiple: true,
        ticket_price,
        payment_token,
        prize_amount: ticket_price * max_tickets as i128,
        prizes: vec![env, 10_000u32],
        randomness_source: RandomnessSource::Internal,
        oracle_address: None,
        protocol_fee_bp: 0,
        treasury_address: None,
        swap_router: None,
        tikka_token: None,
        metadata_hash: BytesN::from_array(env, &[1; 32]),
        claim_lockup_seconds: Some(0),
        swap_deadline_seconds: Some(0),
        early_bird_ticket_percentage: early_bird_percentage,
        early_bird_discount_bp: early_bird_discount_bp,
        category: None,
        unique_winners: false,
        bundles: Vec::new(env),
        prize_token: None,
        nft_contract: None,
    };

    client.init(&factory, &admin, &creator, &config);
    env.as_contract(&contract_id, || env.storage().instance().remove(&DataKey::Factory));
    client.deposit_prize();

    TicketSetup {
        client,
        contract_id,
        admin,
        creator,
        buyer,
        recipient,
        token,
    }
}

/// Calculate expected price for a ticket purchase accounting for early-bird pricing.
///
/// This independent calculation must NOT call calculate_buy_quote to avoid
/// circular validation where a broken shared helper makes both paths appear correct.
fn calculate_expected_price(
    quantity: u32,
    ticket_price: i128,
    tickets_sold: u32,
    max_tickets: u32,
    early_bird_percentage: u32,
    early_bird_discount_bp: u32,
) -> i128 {
    if early_bird_percentage == 0 || early_bird_discount_bp == 0 {
        // No early-bird discount
        return ticket_price * quantity as i128;
    }

    let early_bird_cap = (max_tickets as u64 * early_bird_percentage as u64 / 100) as u32;
    let remaining_discounted = early_bird_cap.saturating_sub(tickets_sold);

    if remaining_discounted == 0 {
        // All tickets at regular price
        return ticket_price * quantity as i128;
    }

    let discounted_count = remaining_discounted.min(quantity);
    let regular_count = quantity - discounted_count;

    // Calculate discounted price
    // discount_bp = 10000 means 100% discount (free)
    // discounted_price = ticket_price * (10000 - discount_bp) / 10000
    let discounted_price = ticket_price * (10000 - early_bird_discount_bp as i128) / 10000;
    
    // Ensure discounted price respects MIN_TICKET_PRICE
    let discounted_price = discounted_price.max(MIN_TICKET_PRICE);

    discounted_count as i128 * discounted_price + regular_count as i128 * ticket_price
}

#[test]
fn preview_matches_buy_tickets_no_early_bird() {
    let env = Env::default();
    env.mock_all_auths();
    let setup = setup_pricing_test(&env, 100, MIN_TICKET_PRICE, 0, 0);

    let quantity = 10u32;
    let balance_before = setup.token.balance(&setup.buyer);
    
    // Get the preview quote
    let preview = setup.client.preview_buy(&quantity);
    
    // Execute the purchase
    setup.client.buy_tickets(&setup.buyer, &quantity);
    
    let balance_after = setup.token.balance(&setup.buyer);
    let actual_charge = balance_before - balance_after;

    // The documented invariant: preview.total must equal actual charge
    assert_eq!(preview.total, actual_charge,
        "preview_buy quote must match actual buy_tickets charge");
    
    // Also verify against independently calculated expected value
    let expected = calculate_expected_price(quantity, MIN_TICKET_PRICE, 0, 100, 0, 0);
    assert_eq!(actual_charge, expected,
        "actual charge must match independently calculated price");
}

#[test]
fn preview_matches_buy_tickets_before_early_bird_window() {
    let env = Env::default();
    env.mock_all_auths();
    
    // Early bird covers first 30% of tickets with 20% discount
    let setup = setup_pricing_test(&env, 100, MIN_TICKET_PRICE * 10, 30, 2000);

    let quantity = 10u32;
    let balance_before = setup.token.balance(&setup.buyer);
    
    let preview = setup.client.preview_buy(&quantity);
    setup.client.buy_tickets(&setup.buyer, &quantity);
    
    let balance_after = setup.token.balance(&setup.buyer);
    let actual_charge = balance_before - balance_after;

    assert_eq!(preview.total, actual_charge,
        "preview_buy must match buy_tickets during early-bird window");
    
    // Calculate expected: all 10 tickets should get 20% discount
    // discounted_price = MIN_TICKET_PRICE * 10 * 0.8 = MIN_TICKET_PRICE * 8
    let expected = calculate_expected_price(quantity, MIN_TICKET_PRICE * 10, 0, 100, 30, 2000);
    assert_eq!(actual_charge, expected,
        "actual charge must match independently calculated early-bird price");
}

#[test]
fn preview_matches_buy_tickets_during_early_bird_window() {
    let env = Env::default();
    env.mock_all_auths();
    
    // Early bird covers first 30% (30 tickets) with 25% discount
    let setup = setup_pricing_test(&env, 100, MIN_TICKET_PRICE * 10, 30, 2500);

    // First purchase: 15 tickets (all discounted)
    setup.client.buy_tickets(&setup.buyer, &15);
    
    // Second purchase: 10 tickets (all still discounted, 15+10=25 < 30)
    let quantity = 10u32;
    let balance_before = setup.token.balance(&setup.buyer);
    
    let preview = setup.client.preview_buy(&quantity);
    setup.client.buy_tickets(&setup.buyer, &quantity);
    
    let balance_after = setup.token.balance(&setup.buyer);
    let actual_charge = balance_before - balance_after;

    assert_eq!(preview.total, actual_charge,
        "preview_buy must match buy_tickets in middle of early-bird window");
    
    let expected = calculate_expected_price(quantity, MIN_TICKET_PRICE * 10, 15, 100, 30, 2500);
    assert_eq!(actual_charge, expected,
        "actual charge must match independently calculated price");
}

#[test]
fn preview_matches_buy_tickets_straddling_early_bird_boundary() {
    let env = Env::default();
    env.mock_all_auths();
    
    // Early bird covers first 30% (30 tickets) with 30% discount
    let setup = setup_pricing_test(&env, 100, MIN_TICKET_PRICE * 10, 30, 3000);

    // Purchase 27 tickets first
    setup.client.buy_tickets(&setup.buyer, &27);
    
    // Now buy 10 more: 3 should be discounted, 7 at regular price
    let quantity = 10u32;
    let balance_before = setup.token.balance(&setup.buyer);
    
    let preview = setup.client.preview_buy(&quantity);
    setup.client.buy_tickets(&setup.buyer, &quantity);
    
    let balance_after = setup.token.balance(&setup.buyer);
    let actual_charge = balance_before - balance_after;

    assert_eq!(preview.total, actual_charge,
        "preview_buy must match buy_tickets when straddling early-bird boundary");
    
    // Calculate expected independently:
    // 3 tickets at discounted price: MIN_TICKET_PRICE * 10 * 0.7
    // 7 tickets at regular price: MIN_TICKET_PRICE * 10
    let expected = calculate_expected_price(quantity, MIN_TICKET_PRICE * 10, 27, 100, 30, 3000);
    assert_eq!(actual_charge, expected,
        "straddle case must charge exact expected split price");
}

#[test]
fn preview_matches_buy_tickets_after_early_bird_window() {
    let env = Env::default();
    env.mock_all_auths();
    
    // Early bird covers first 20% (20 tickets) with 15% discount
    let setup = setup_pricing_test(&env, 100, MIN_TICKET_PRICE * 10, 20, 1500);

    // Purchase all early-bird tickets
    setup.client.buy_tickets(&setup.buyer, &20);
    
    // Now buy after early-bird window ends
    let quantity = 10u32;
    let balance_before = setup.token.balance(&setup.buyer);
    
    let preview = setup.client.preview_buy(&quantity);
    setup.client.buy_tickets(&setup.buyer, &quantity);
    
    let balance_after = setup.token.balance(&setup.buyer);
    let actual_charge = balance_before - balance_after;

    assert_eq!(preview.total, actual_charge,
        "preview_buy must match buy_tickets after early-bird window");
    
    // All tickets at regular price
    let expected = calculate_expected_price(quantity, MIN_TICKET_PRICE * 10, 20, 100, 20, 1500);
    assert_eq!(actual_charge, expected,
        "actual charge must be regular price after early-bird window");
}

#[test]
fn preview_matches_buy_tickets_zero_early_bird_percentage() {
    let env = Env::default();
    env.mock_all_auths();
    
    // early_bird_ticket_percentage = 0 means no early-bird discount
    let setup = setup_pricing_test(&env, 100, MIN_TICKET_PRICE * 10, 0, 5000);

    let quantity = 10u32;
    let balance_before = setup.token.balance(&setup.buyer);
    
    let preview = setup.client.preview_buy(&quantity);
    setup.client.buy_tickets(&setup.buyer, &quantity);
    
    let balance_after = setup.token.balance(&setup.buyer);
    let actual_charge = balance_before - balance_after;

    assert_eq!(preview.total, actual_charge,
        "preview_buy must match buy_tickets when early_bird_percentage is 0");
    
    // Should be flat pricing
    let expected = MIN_TICKET_PRICE * 10 * quantity as i128;
    assert_eq!(actual_charge, expected,
        "zero percentage means no discount applied");
}

#[test]
fn preview_matches_buy_tickets_hundred_percent_early_bird() {
    let env = Env::default();
    env.mock_all_auths();
    
    // 100% of tickets are early-bird with 40% discount
    let setup = setup_pricing_test(&env, 100, MIN_TICKET_PRICE * 10, 100, 4000);

    let quantity = 50u32;
    let balance_before = setup.token.balance(&setup.buyer);
    
    let preview = setup.client.preview_buy(&quantity);
    setup.client.buy_tickets(&setup.buyer, &quantity);
    
    let balance_after = setup.token.balance(&setup.buyer);
    let actual_charge = balance_before - balance_after;

    assert_eq!(preview.total, actual_charge,
        "preview_buy must match buy_tickets when all tickets are early-bird");
    
    // All tickets should be discounted
    let expected = calculate_expected_price(quantity, MIN_TICKET_PRICE * 10, 0, 100, 100, 4000);
    assert_eq!(actual_charge, expected,
        "100% early-bird means all tickets get discount");
}

#[test]
fn preview_matches_buy_tickets_zero_discount_bp() {
    let env = Env::default();
    env.mock_all_auths();
    
    // Early-bird window exists but 0% discount
    let setup = setup_pricing_test(&env, 100, MIN_TICKET_PRICE * 10, 50, 0);

    let quantity = 10u32;
    let balance_before = setup.token.balance(&setup.buyer);
    
    let preview = setup.client.preview_buy(&quantity);
    setup.client.buy_tickets(&setup.buyer, &quantity);
    
    let balance_after = setup.token.balance(&setup.buyer);
    let actual_charge = balance_before - balance_after;

    assert_eq!(preview.total, actual_charge,
        "preview_buy must match buy_tickets when discount_bp is 0");
    
    // Zero discount means regular price
    let expected = MIN_TICKET_PRICE * 10 * quantity as i128;
    assert_eq!(actual_charge, expected,
        "zero discount_bp means no discount applied");
}

#[test]
fn preview_matches_buy_tickets_max_discount_respects_min_price() {
    let env = Env::default();
    env.mock_all_auths();
    
    // 100% discount (free tickets) but must respect MIN_TICKET_PRICE
    let setup = setup_pricing_test(&env, 100, MIN_TICKET_PRICE * 2, 50, 10000);

    let quantity = 10u32;
    let balance_before = setup.token.balance(&setup.buyer);
    
    let preview = setup.client.preview_buy(&quantity);
    setup.client.buy_tickets(&setup.buyer, &quantity);
    
    let balance_after = setup.token.balance(&setup.buyer);
    let actual_charge = balance_before - balance_after;

    assert_eq!(preview.total, actual_charge,
        "preview_buy must match buy_tickets with 100% discount");
    
    // 100% discount should result in MIN_TICKET_PRICE per ticket
    let expected = calculate_expected_price(quantity, MIN_TICKET_PRICE * 2, 0, 100, 50, 10000);
    assert_eq!(actual_charge, expected,
        "100% discount must still charge MIN_TICKET_PRICE per ticket");
}

#[test]
fn preview_matches_buy_tickets_for_no_early_bird() {
    let env = Env::default();
    env.mock_all_auths();
    let setup = setup_pricing_test(&env, 100, MIN_TICKET_PRICE, 0, 0);

    let quantity = 10u32;
    let balance_before = setup.token.balance(&setup.buyer);
    
    let preview = setup.client.preview_buy(&quantity);
    setup.client.buy_tickets_for(&setup.buyer, &setup.recipient, &quantity);
    
    let balance_after = setup.token.balance(&setup.buyer);
    let actual_charge = balance_before - balance_after;

    assert_eq!(preview.total, actual_charge,
        "preview_buy must match buy_tickets_for charge");
}

#[test]
fn preview_matches_buy_tickets_for_with_early_bird() {
    let env = Env::default();
    env.mock_all_auths();
    
    // Early bird: 40% of tickets with 25% discount
    let setup = setup_pricing_test(&env, 100, MIN_TICKET_PRICE * 10, 40, 2500);

    let quantity = 20u32;
    let balance_before = setup.token.balance(&setup.buyer);
    
    let preview = setup.client.preview_buy(&quantity);
    setup.client.buy_tickets_for(&setup.buyer, &setup.recipient, &quantity);
    
    let balance_after = setup.token.balance(&setup.buyer);
    let actual_charge = balance_before - balance_after;

    assert_eq!(preview.total, actual_charge,
        "preview_buy must match buy_tickets_for with early-bird pricing");
    
    let expected = calculate_expected_price(quantity, MIN_TICKET_PRICE * 10, 0, 100, 40, 2500);
    assert_eq!(actual_charge, expected,
        "buy_tickets_for must use same pricing as buy_tickets");
}

#[test]
fn preview_matches_buy_tickets_for_straddling_boundary() {
    let env = Env::default();
    env.mock_all_auths();
    
    // Early bird: 30% of tickets (30 total) with 20% discount
    let setup = setup_pricing_test(&env, 100, MIN_TICKET_PRICE * 10, 30, 2000);

    // First buy 25 tickets
    setup.client.buy_tickets(&setup.buyer, &25);
    
    // Now gift 10 tickets: 5 discounted, 5 regular
    let quantity = 10u32;
    let balance_before = setup.token.balance(&setup.buyer);
    
    let preview = setup.client.preview_buy(&quantity);
    setup.client.buy_tickets_for(&setup.buyer, &setup.recipient, &quantity);
    
    let balance_after = setup.token.balance(&setup.buyer);
    let actual_charge = balance_before - balance_after;

    assert_eq!(preview.total, actual_charge,
        "preview_buy must match buy_tickets_for when straddling boundary");
    
    let expected = calculate_expected_price(quantity, MIN_TICKET_PRICE * 10, 25, 100, 30, 2000);
    assert_eq!(actual_charge, expected,
        "buy_tickets_for must apply split pricing correctly");
}

#[test]
fn property_test_preview_matches_actual_for_valid_quantities() {
    let env = Env::default();
    env.mock_all_auths();
    
    // Test with early-bird pricing: 50% tickets, 30% discount
    let setup = setup_pricing_test(&env, 50, MIN_TICKET_PRICE * 10, 50, 3000);

    // Test all valid quantities from 1 to max_tickets_per_tx
    // This is a deterministic property test covering the valid range
    for quantity in 1..=50 {
        let buyer_before = setup.token.balance(&setup.buyer);
        
        let preview = setup.client.preview_buy(&quantity);
        setup.client.buy_tickets(&setup.buyer, &quantity);
        
        let buyer_after = setup.token.balance(&setup.buyer);
        let actual_charge = buyer_before - buyer_after;

        assert_eq!(preview.total, actual_charge,
            "preview_buy must match buy_tickets for quantity {}", quantity);
        
        // Calculate tickets_sold before this purchase
        let tickets_sold_before = if quantity == 1 { 0 } else { (quantity - 1) * quantity / 2 };
        let expected = calculate_expected_price(
            1, 
            MIN_TICKET_PRICE * 10, 
            tickets_sold_before,
            50,
            50,
            3000
        );
        assert_eq!(actual_charge, expected,
            "actual charge must match expected for quantity {}", quantity);
    }
}
