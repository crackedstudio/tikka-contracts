import re

with open('contracts/raffle/src/instance/mod.rs', 'r') as f:
    content = f.read()

# 1. Add build_internal_seed and do_finalize_with_seed helpers
helpers = """
fn build_internal_seed(env: &Env) -> u64 {
    let xdr = (
        env.ledger().timestamp(),
        env.ledger().sequence(),
        env.current_contract_address(),
    )
        .to_xdr(env);
    let hash = env.crypto().sha256(&xdr);
    
    // Convert first 8 bytes of hash to u64
    let mut bytes = [0u8; 8];
    for i in 0..8 {
        bytes[i] = hash.get(i as u32).unwrap();
    }
    u64::from_be_bytes(bytes)
}

fn do_finalize_with_seed(
    env: &Env,
    mut raffle: Raffle,
    seed: u64,
    randomness_type: RandomnessType,
) -> Result<(), Error> {
    let total_tickets = get_ticket_count(env);
    if total_tickets == 0 {
        return Err(Error::NoTicketsSold);
    }

    let selector = OracleSeedWinnerSelection::new(seed);
    let winning_ticket_ids =
        selector.select_winner_indices(env, total_tickets, raffle.prizes.len() as u32);
    let mut winners = Vec::new(env);

    for i in 0..winning_ticket_ids.len() {
        let winner_index = winning_ticket_ids.get(i).unwrap();
        let ticket_id = winner_index + 1;
        let winner = get_ticket_owner(env, ticket_id).ok_or(Error::TicketNotFound)?;
        winners.push_back(winner.clone());

        env.events().publish(
            (
                Symbol::new(env, "WinnerDrawn"),
                winner.clone(),
                winner_index,
            ),
            WinnerDrawn {
                winner: winner.clone(),
                ticket_id: winner_index,
                tier_index: i,
                timestamp: env.ledger().timestamp(),
            },
        );
    }

    let mut claimed_winners = Vec::new(env);
    for _ in 0..raffle.prizes.len() {
        claimed_winners.push_back(false);
    }

    let fairness_metadata = FairnessMetadata {
        seed,
        randomness_source: raffle.randomness_source.clone(),
        winning_ticket_indices: winning_ticket_ids.clone(),
        draw_timestamp: env.ledger().timestamp(),
        draw_sequence: env.ledger().sequence(),
    };
    env.storage()
        .instance()
        .set(&DataKey::RandomnessSeed, &fairness_metadata);

    raffle.status = RaffleStatus::Finalized;
    raffle.winners = winners.clone();
    raffle.claimed_winners = claimed_winners;
    raffle.finalized_at = Some(env.ledger().timestamp());
    write_raffle(env, &raffle);

    if !env.storage().persistent().has(&DataKey::FinishTime) {
        env.storage()
            .persistent()
            .set(&DataKey::FinishTime, &env.ledger().timestamp());
    }

    publish_event(
        env,
        "raffle_finalized",
        RaffleFinalized {
            winners,
            winning_ticket_ids,
            total_tickets_sold: raffle.tickets_sold,
            randomness_source: raffle.randomness_source.clone(),
            randomness_type,
            finalized_at: env.ledger().timestamp(),
        },
    );

    publish_event(
        env,
        "status_changed",
        RaffleStatusChanged {
            old_status: RaffleStatus::Drawing,
            new_status: RaffleStatus::Finalized,
            timestamp: env.ledger().timestamp(),
        },
    );

    Ok(())
}
"""

# Insert helpers after require_not_paused
content = content.replace('fn require_not_paused(env: &Env) -> Result<(), Error> {', 'fn require_not_paused(env: &Env) -> Result<(), Error> {')
content = re.sub(r'fn require_not_paused\(env: &Env\) -> Result<\(\), Error> \{.*?\}', r'fn require_not_paused(env: &Env) -> Result<(), Error> {\n    if env.storage().instance().get(&DataKey::Paused).unwrap_or(false) {\n        return Err(Error::ContractPaused);\n    }\n    Ok(())\n}\n' + helpers, content, flags=re.DOTALL)

# 2. Refactor finalize_raffle
content = re.sub(r'pub fn finalize_raffle\(env: Env\) -> Result<\(\), Error> \{.*?\}', 
r"""    pub fn finalize_raffle(env: Env) -> Result<(), Error> {
        require_creator(&env)?;
        let mut raffle = read_raffle(&env)?;

        if raffle.status == RaffleStatus::Active {
            if (raffle.end_time != 0 && env.ledger().timestamp() >= raffle.end_time)
                || raffle.tickets_sold >= raffle.max_tickets
            {
                raffle.status = RaffleStatus::Drawing;
                publish_event(
                    &env,
                    "status_changed",
                    RaffleStatusChanged {
                        old_status: RaffleStatus::Active,
                        new_status: RaffleStatus::Drawing,
                        timestamp: env.ledger().timestamp(),
                    },
                );
            }
        }

        if raffle.status != RaffleStatus::Drawing {
            return Err(Error::InvalidStateTransition);
        }

        if raffle.tickets_sold == 0 {
            return Err(Error::NoTicketsSold);
        }

        if raffle.min_tickets > 0 && raffle.tickets_sold < raffle.min_tickets {
            raffle.status = RaffleStatus::Failed;
            write_raffle(&env, &raffle);
            publish_event(
                &env,
                "status_changed",
                RaffleStatusChanged {
                    old_status: RaffleStatus::Drawing,
                    new_status: RaffleStatus::Failed,
                    timestamp: env.ledger().timestamp(),
                },
            );
            return Ok(());
        }

        publish_event(
            &env,
            "draw_triggered",
            DrawTriggered {
                triggered_by: raffle.creator.clone(),
                total_tickets_sold: raffle.tickets_sold,
                timestamp: env.ledger().timestamp(),
            },
        );

        if raffle.randomness_source == RandomnessSource::External {
            let oracle = raffle
                .oracle_address
                .as_ref()
                .ok_or(Error::OracleNotSet)?
                .clone();

            write_raffle(&env, &raffle);

            let already: bool = env
                .storage()
                .instance()
                .get(&DataKey::RandomnessRequested)
                .unwrap_or(false);
            if already {
                return Err(Error::RandomnessAlreadyRequested);
            }
            env.storage()
                .instance()
                .set(&DataKey::RandomnessRequested, &true);
            env.storage()
                .instance()
                .set(&DataKey::RandomnessRequestLedger, &env.ledger().sequence());

            publish_event(
                &env,
                "randomness_requested",
                RandomnessRequested {
                    oracle,
                    timestamp: env.ledger().timestamp(),
                },
            );
            return Ok(());
        }

        let seed = build_internal_seed(&env);
        do_finalize_with_seed(&env, raffle, seed, RandomnessType::Prng)
    }""", content, flags=re.DOTALL)

# 3. Refactor provide_randomness
content = re.sub(r'pub fn provide_randomness\(.*?\) -> Result<Address, Error> \{.*?\}',
r"""    pub fn provide_randomness(
        env: Env,
        random_seed: u64,
        public_key: BytesN<32>,
        proof: BytesN<64>,
    ) -> Result<Address, Error> {
        let mut raffle = read_raffle(&env)?;

        let oracle = match &raffle.oracle_address {
            Some(addr) => {
                addr.require_auth();
                addr.clone()
            }
            None => return Err(Error::OracleNotSet),
        };

        if raffle.status != RaffleStatus::Drawing {
            return Err(Error::InvalidStateTransition);
        }
        if raffle.randomness_source != RandomnessSource::External {
            return Err(Error::InvalidStateTransition);
        }

        let request_pending: bool = env
            .storage()
            .instance()
            .get(&DataKey::RandomnessRequested)
            .unwrap_or(false);
        if !request_pending {
            return Err(Error::NoRandomnessRequest);
        }

        verify_randomness_proof_internal(&env, &public_key, random_seed, &proof);

        env.storage()
            .instance()
            .remove(&DataKey::RandomnessRequested);
        env.storage()
            .instance()
            .remove(&DataKey::RandomnessRequestLedger);

        do_finalize_with_seed(&env, raffle, random_seed, RandomnessType::Vrf)?;
        Ok(env.current_contract_address())
    }""", content, flags=re.DOTALL)

with open('contracts/raffle/src/instance/mod.rs', 'w') as f:
    f.write(content)
