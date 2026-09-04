//! Read-only query surface for the RaffleFactory contract.
//!
//! All functions in this module are pure views: they read storage but never
//! write to it.  Every paginated view clamps its `limit` via
//! [`raffle_shared::effective_limit`] and returns a [`PageResultRaffles`]
//! with `total` and `has_more` fields.

use soroban_sdk::{Address, Env, Symbol, IntoVal, Vec};

use crate::{ContractError, DataKey, ProtocolStats, RaffleFactory, StateCheckpoint};
use raffle_shared::{
    effective_limit, FairnessData, PageResultRaffles, PaginationParams,
};

/// Shared pagination helper: slice a `Vec<Address>` with offset/limit and
/// compute `total` and `has_more`.
fn paginate_raffles(
    env: &Env,
    source: &Vec<Address>,
    params: &PaginationParams,
) -> PageResultRaffles {
    let total = source.len();
    let lim = effective_limit(params.limit);
    let offset = params.offset;

    if offset >= total {
        return PageResultRaffles {
            items: Vec::new(env),
            total,
            has_more: false,
        };
    }

    let end = offset.saturating_add(lim).min(total);
    let mut items: Vec<Address> = Vec::new(env);
    for i in offset..end {
        if let Some(addr) = source.get(i) {
            items.push_back(addr);
        }
    }
    let has_more = end < total;
    PageResultRaffles {
        items,
        total,
        has_more,
    }
}

#[contractimpl]
impl RaffleFactory {
    // ── Protocol-level stats ──────────────────────────────────────────────

    /// Return aggregate protocol statistics: total raffles created, fee
    /// basis-points, paused flag, and unique participant count.
    pub fn get_protocol_stats(env: Env) -> ProtocolStats {
        let total_raffles_created: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalRafflesCreated)
            .unwrap_or(0);
        let protocol_fee_bp: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::ProtocolFeeBP)
            .unwrap_or(0);
        let paused: bool = env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false);
        let total_unique_participants: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::TotalUniqueParticipants)
            .unwrap_or(0);

        ProtocolStats {
            total_raffles_created,
            protocol_fee_bp,
            paused,
            total_unique_participants,
        }
    }

    // ── Single-record lookups ─────────────────────────────────────────────

    /// O(1) direct lookup of a raffle address by its stable ID.
    pub fn get_raffle_by_id(env: Env, raffle_id: u32) -> Option<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::RaffleById(raffle_id))
    }

    /// Returns the stable ID that will be assigned to the next raffle.
    pub fn get_next_raffle_id(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::NextRaffleId)
            .unwrap_or(0u32)
    }

    /// Predict the deterministic contract address for a raffle before
    /// deployment.
    pub fn predict_raffle_address(env: Env, creator: Address, nonce: u64) -> Address {
        let salt = crate::compute_raffle_salt(&env, &creator, nonce);
        env.deployer()
            .with_current_contract(salt)
            .deployed_address()
    }

    /// Returns the current count of live (non-tombstoned) raffles.
    pub fn get_raffle_count(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::RaffleCount)
            .unwrap_or(0u32)
    }

    /// Return the cumulative ticket-sale volume for a specific `asset` token.
    pub fn get_total_volume(env: Env, asset: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::TotalVolumePerAsset(asset))
            .unwrap_or(0)
    }

    /// Return the current admin address.
    pub fn get_admin(env: Env) -> Result<Address, ContractError> {
        env.storage()
            .persistent()
            .get(&DataKey::Admin)
            .ok_or(ContractError::NotAuthorized)
    }

    // ── Paginated list views ──────────────────────────────────────────────

    /// Return a paginated slice of all live raffle addresses.
    pub fn get_raffles_page(env: Env, params: PaginationParams) -> PageResultRaffles {
        let next_id: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::NextRaffleId)
            .unwrap_or(0u32);

        let lim = effective_limit(params.limit);
        let offset = params.offset;

        let total: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::RaffleCount)
            .unwrap_or(0u32);

        if offset >= next_id {
            return PageResultRaffles {
                items: Vec::new(&env),
                total,
                has_more: false,
            };
        }

        let end = offset.saturating_add(lim).min(next_id);
        let mut items: Vec<Address> = Vec::new(&env);
        for id in offset..end {
            if let Some(addr) = env
                .storage()
                .persistent()
                .get::<_, Address>(&DataKey::RaffleById(id))
            {
                items.push_back(addr);
            }
        }

        let has_more = end < next_id;
        PageResultRaffles {
            items,
            total,
            has_more,
        }
    }

    /// Return a paginated list of raffle addresses created by `creator`.
    pub fn get_raffles_by_creator(
        env: Env,
        creator: Address,
        params: PaginationParams,
    ) -> PageResultRaffles {
        let creator_raffles: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::CreatorRaffles(creator))
            .unwrap_or_else(|| Vec::new(&env));

        paginate_raffles(&env, &creator_raffles, &params)
    }

    /// Return a paginated list of raffle addresses tagged with `category`.
    pub fn get_raffles_by_category(
        env: Env,
        category: soroban_sdk::String,
        params: PaginationParams,
    ) -> PageResultRaffles {
        let category_raffles: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::CategoryRaffles(category))
            .unwrap_or_else(|| Vec::new(&env));

        paginate_raffles(&env, &category_raffles, &params)
    }

    // ── Checkpoints ───────────────────────────────────────────────────────

    /// Return a state checkpoint by its 1-based index.
    pub fn get_checkpoint(env: Env, index: u32) -> Option<StateCheckpoint> {
        env.storage().persistent().get(&DataKey::Checkpoint(index))
    }

    /// Return the index of the most recently created checkpoint.
    pub fn get_latest_checkpoint_index(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::LatestCheckpointIndex)
            .unwrap_or(0u32)
    }

    // ── Participants & fairness ───────────────────────────────────────────

    /// Return the total number of unique participants across all raffles.
    pub fn get_unique_participants(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::TotalUniqueParticipants)
            .unwrap_or(0)
    }

    /// Retrieve the fairness data for a specific raffle instance.
    pub fn get_raffle_fairness_data(
        env: Env,
        raffle_id: Address,
    ) -> Result<FairnessData, ContractError> {
        Ok(env.invoke_contract::<FairnessData>(
            &raffle_id,
            &Symbol::new(&env, "get_fairness_data"),
            ().into_val(&env),
        ))
    }
}
