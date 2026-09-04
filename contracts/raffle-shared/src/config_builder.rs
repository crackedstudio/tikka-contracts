#![cfg(any(test, feature = "testutils"))]

use soroban_sdk::{Address, BytesN, Env, String, Vec};
use crate::{RandomnessSource, RaffleConfig};

/// Fluent builder for [`RaffleConfig`] intended for tests and development.
///
/// Production callers should continue constructing [`RaffleConfig`] exhaustively
/// so that every new field is a deliberate decision at the factory boundary.
///
/// # Example
///
/// ```rust
/// let config = RaffleConfigBuilder::new(&env, payment_token)
///     .max_tickets(100)
///     .ticket_price(10_000)
///     .prizes(vec![&env, 10_000])
///     .build();
/// ```
#[allow(dead_code)]
pub struct RaffleConfigBuilder<'a> {
    env: &'a Env,
    payment_token: Address,
    description: String,
    end_time: u64,
    no_deadline: bool,
    max_tickets: u32,
    max_tickets_per_tx: Option<u32>,
    max_tickets_per_address: u32,
    min_tickets: u32,
    allow_multiple: bool,
    ticket_price: i128,
    prize_amount: i128,
    prizes: Vec<u32>,
    randomness_source: RandomnessSource,
    oracle_address: Option<Address>,
    protocol_fee_bp: u32,
    treasury_address: Option<Address>,
    swap_router: Option<Address>,
    tikka_token: Option<Address>,
    metadata_hash: BytesN<32>,
    claim_lockup_seconds: u64,
    claim_expiry_seconds: Option<u64>,
    swap_deadline_seconds: u64,
    early_bird_ticket_percentage: u32,
    early_bird_discount_bp: u32,
    category: Option<String>,
}

impl<'a> RaffleConfigBuilder<'a> {
    /// Create a new builder with the given environment and payment token.
    ///
    /// All other fields receive safe defaults that satisfy `raffle-instance`
    /// validation.
    pub fn new(env: &'a Env, payment_token: Address) -> Self {
        Self {
            env,
            payment_token,
            description: String::from_str(env, "Test Raffle"),
            end_time: 0,
            no_deadline: true,
            max_tickets: 100,
            max_tickets_per_tx: None,
            max_tickets_per_address: 0,
            min_tickets: 1,
            allow_multiple: true,
            ticket_price: 10_000,
            prize_amount: 10_000,
            prizes: Vec::new(env),
            randomness_source: RandomnessSource::Internal,
            oracle_address: None,
            protocol_fee_bp: 0,
            treasury_address: None,
            swap_router: None,
            tikka_token: None,
            metadata_hash: BytesN::from_array(env, &[1u8; 32]),
            claim_lockup_seconds: 0,
            claim_expiry_seconds: None,
            swap_deadline_seconds: 0,
            early_bird_ticket_percentage: 0,
            early_bird_discount_bp: 0,
            category: None,
        }
    }

    pub fn description(mut self, description: String) -> Self {
        self.description = description;
        self
    }

    pub fn end_time(mut self, end_time: u64) -> Self {
        self.end_time = end_time;
        self
    }

    pub fn no_deadline(mut self, no_deadline: bool) -> Self {
        self.no_deadline = no_deadline;
        self
    }

    pub fn max_tickets(mut self, max_tickets: u32) -> Self {
        self.max_tickets = max_tickets;
        self
    }

    pub fn max_tickets_per_tx(mut self, max_tickets_per_tx: u32) -> Self {
        self.max_tickets_per_tx = Some(max_tickets_per_tx);
        self
    }

    pub fn min_tickets(mut self, min_tickets: u32) -> Self {
        self.min_tickets = min_tickets;
        self
    }

    pub fn allow_multiple(mut self, allow_multiple: bool) -> Self {
        self.allow_multiple = allow_multiple;
        self
    }

    pub fn ticket_price(mut self, ticket_price: i128) -> Self {
        self.ticket_price = ticket_price;
        self
    }

    pub fn prize_amount(mut self, prize_amount: i128) -> Self {
        self.prize_amount = prize_amount;
        self
    }

    pub fn prizes(mut self, prizes: Vec<u32>) -> Self {
        self.prizes = prizes;
        self
    }

    pub fn randomness_source(mut self, randomness_source: RandomnessSource) -> Self {
        self.randomness_source = randomness_source;
        self
    }

    pub fn oracle_address(mut self, oracle_address: Option<Address>) -> Self {
        self.oracle_address = oracle_address;
        self
    }

    pub fn protocol_fee_bp(mut self, protocol_fee_bp: u32) -> Self {
        self.protocol_fee_bp = protocol_fee_bp;
        self
    }

    pub fn treasury_address(mut self, treasury_address: Option<Address>) -> Self {
        self.treasury_address = treasury_address;
        self
    }

    pub fn swap_router(mut self, swap_router: Option<Address>) -> Self {
        self.swap_router = swap_router;
        self
    }

    pub fn tikka_token(mut self, tikka_token: Option<Address>) -> Self {
        self.tikka_token = tikka_token;
        self
    }

    pub fn metadata_hash(mut self, metadata_hash: BytesN<32>) -> Self {
        self.metadata_hash = metadata_hash;
        self
    }

    pub fn claim_lockup_seconds(mut self, claim_lockup_seconds: u64) -> Self {
        self.claim_lockup_seconds = claim_lockup_seconds;
        self
    }

    pub fn swap_deadline_seconds(mut self, swap_deadline_seconds: u64) -> Self {
        self.swap_deadline_seconds = swap_deadline_seconds;
        self
    }

    pub fn early_bird_ticket_percentage(mut self, early_bird_ticket_percentage: u32) -> Self {
        self.early_bird_ticket_percentage = early_bird_ticket_percentage;
        self
    }

    pub fn early_bird_discount_bp(mut self, early_bird_discount_bp: u32) -> Self {
        self.early_bird_discount_bp = early_bird_discount_bp;
        self
    }

    pub fn claim_expiry_seconds(mut self, claim_expiry_seconds: u64) -> Self {
        self.claim_expiry_seconds = Some(claim_expiry_seconds);
        self
    }

    pub fn category(mut self, category: Option<String>) -> Self {
        self.category = category;
        self
    }

    /// Build the [`RaffleConfig`].
    ///
    /// # Panics
    ///
    /// Panics if `max_tickets_per_tx` was not set and `max_tickets` is zero,
    /// or if `prizes` is empty.  These conditions indicate a misconfigured
    /// test rather than a runtime failure.
    pub fn build(self) -> RaffleConfig {
        let max_tickets_per_tx = self
            .max_tickets_per_tx
            .unwrap_or_else(|| self.max_tickets.max(1));

        RaffleConfig {
            description: self.description,
            end_time: self.end_time,
            no_deadline: self.no_deadline,
            max_tickets: self.max_tickets,
            max_tickets_per_tx,
            max_tickets_per_address: self.max_tickets_per_address,
            min_tickets: self.min_tickets,
            allow_multiple: self.allow_multiple,
            ticket_price: self.ticket_price,
            payment_token: self.payment_token,
            prize_amount: self.prize_amount,
            prizes: self.prizes,
            randomness_source: self.randomness_source,
            oracle_address: self.oracle_address,
            protocol_fee_bp: self.protocol_fee_bp,
            treasury_address: self.treasury_address,
            swap_router: self.swap_router,
            tikka_token: self.tikka_token,
            metadata_hash: self.metadata_hash,
            claim_lockup_seconds: Some(self.claim_lockup_seconds),
            claim_expiry_seconds: self.claim_expiry_seconds,
            swap_deadline_seconds: self.swap_deadline_seconds,
            early_bird_ticket_percentage: self.early_bird_ticket_percentage,
            early_bird_discount_bp: self.early_bird_discount_bp,
            category: self.category,
        }
    }
}
