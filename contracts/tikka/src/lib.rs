pub fn set_config(env: Env, new_config: Config) {
    let admin = admin::get_admin(&env);
    admin::require_admin(&env, &admin);

    // === CRITICAL FIX: Add upper bound check ===
    require!(
        new_config.protocol_fee_bp <= 10_000,
        ContractError::InvalidProtocolFee
    );

    // Optional: Warn on high fees (e.g., > 20%)
    if new_config.protocol_fee_bp > 2_000 {
        env.events().publish(
            (symbol_short!("high_fee"),),
            new_config.protocol_fee_bp,
        );
    }

    storage::set_config(&env, &new_config);

    env.events().publish(
        (symbol_short!("config"), symbol_short!("updated")),
        new_config,
    );
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum ContractError {
    // ... existing errors
    InvalidProtocolFee = 120,
    // ...
}