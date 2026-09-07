use super::*;
use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
use soroban_sdk::IntoVal;

// --------------------------------------------------------------------------
// Pause precedence matrix (see contracts/raffle-factory/src/pause.rs)
//
// Flag            | Blocks create_raffle | Blocks ticket sales on existing instances
// ----------------+---------------------+------------------------------------
// global pause    | yes                 | yes  (emergency_pause_all -> is_global_paused)
// Factory Paused  | yes                 | no
// CreationPaused  | yes                 | no
//
// `emergency_pause_all` is the single call that halts the protocol.
// ---------------------------------------------------------------------------

#[test]
fn every_factory_admin_entrypoint_succeeds_for_admin() {
    let env = Env::default();
    let (client, _admin, _treasury) = setup_factory(&env);

    for entrypoint in ["pause_factory", "unpause_factory"] {
        match entrypoint {
            "pause_factory" => client.pause_factory(),
            "unpause_factory" => client.unpause_factory(),
            _ => unreachable!(),
        }
    }

    assert!(!client.is_factory_paused());
}

#[test]
fn every_factory_admin_entrypoint_rejects_non_admin() {
    let env = Env::default();
    let (client, _admin, _treasury) = setup_factory(&env);
    let stranger = Address::generate(&env);

    for entrypoint in ["pause_factory", "unpause_factory"] {
        env.mock_auths(&[MockAuth {
            address: &stranger,
            invoke: &MockAuthInvoke {
                contract: &client.address,
                fn_name: entrypoint,
                args: ().into_val(&env),
                sub_invokes: &[],
            },
        }]);

        let result = match entrypoint {
            "pause_factory" => client.try_pause_factory(),
            "unpause_factory" => client.try_unpause_factory(),
            _ => unreachable!(),
        };
        assert!(result.is_err());
    }
}

// --------------------------------------------------------------------------
// Test matrix: each pause flag against its query and access control.
// --------------------------------------------------------------------------

#[test]
fn all_pause_flags_default_to_false() {
    let env = Env::default();
    let (client, _admin, _treasury) = setup_factory(&env);

    assert!(!client.is_factory_paused());
    assert!(!client.is_creation_paused());
    assert!(!client.is_global_paused());
}

#[test]
fn factory_pause_toggles_only_factory_flag() {
    let env = Env::default();
    let (client, _admin, _treasury) = setup_factory(&env);

    client.pause_factory();
    assert!(client.is_factory_paused());
    assert!(!client.is_creation_paused());
    assert!(!client.is_global_paused());

    client.unpause_factory();
    assert!(!client.is_factory_paused());
    // Unpausing the factory must not clear the global pause.
    assert!(!client.is_global_paused());
}

#[test]
fn global_pause_is_independent_of_factory_pause() {
    let env = Env::default();
    let (client, _admin, _treasury) = setup_factory(&env);

    client.emergency_pause_all();
    assert!(client.is_global_paused());
    // global pause does not set the factory-level flag.
    assert!(!client.is_factory_paused());

    client.emergency_unpause_all();
    assert!(!client.is_global_paused());
}

#[test]
fn emergency_and_creation_entrypoints_reject_non_admin() {
    let env = Env::default();
    let (client, _admin, _treasury) = setup_factory(&env);
    let stranger = Address::generate(&env);

    for entrypoint in ["emergency_pause_all", "emergency_unpause_all"] {
        env.mock_auths(&[MockAuth {
            address: &stranger,
            invoke: &MockAuthInvoke {
                contract: &client.address,
                fn_name: entrypoint,
                args: ().into_val(&env),
                sub_invokes: &[],
            },
        }]);

        let result = match entrypoint {
            "emergency_pause_all" => client.try_emergency_pause_all(),
            "emergency_unpause_all" => client.try_emergency_unpause_all(),
            _ => unreachable!(),
        };
        assert!(result.is_err());
    }
}
