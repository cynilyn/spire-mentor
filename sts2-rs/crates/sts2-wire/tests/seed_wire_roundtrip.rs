//! v0.111.0 seed bases across the wire boundary.
//!
//! The run base (`$.rng.run_seed`) and the player base (`$.rng.player_seed`)
//! are both `u64` and differ for `old`-prefixed seed strings, so a combat
//! wire must carry both and the importer must rebuild `RunRngSet::new(run,
//! player)` from them. Vectors: the 0.111.0 oracle dump
//! (sts2_bot `logs/engine/p0/stage1_vectors.json`, sha256
//! 5267538d84c1a1468b68656b82bdd270e4a4e21b60a251366beb475786aa8976):
//! `P0A0` run = player = 10883704133362413376 (above `i64::MAX`), `oldP0A0`
//! run = 273692109, player = 15676685915450073139 (above `i64::MAX`).

use serde_json::{json, Value};
use sts2_core::dotnet_rng::seed_bases;
use sts2_core::run_rng_set::RunRngSet;
use sts2_core::CombatState;
use sts2_wire::emit::combat::combat_state_to_value;
use sts2_wire::state_from_json;

fn state_with_seed(seed: &str, rewards_reads: u32, shuffle_reads: u32) -> CombatState {
    let b = seed_bases(seed);
    let mut state = CombatState::default();
    state.run_rng = RunRngSet::new(b.run_seed, b.player_seed).with_seed_string(seed);
    for _ in 0..rewards_reads {
        let _ = state.run_rng.rewards.next_int(0, 999);
    }
    for _ in 0..shuffle_reads {
        let _ = state.run_rng.shuffle.next_int(0, 999);
    }
    state
}

#[test]
fn old_p0a0_round_trips_both_bases_and_the_next_player_draw() {
    let original = state_with_seed("oldP0A0", 3, 2);
    let wire = combat_state_to_value(&original).expect("emit");
    assert_eq!(wire["rng"]["run_seed"], json!(273_692_109_u64));
    assert_eq!(
        wire["rng"]["player_seed"],
        json!(15_676_685_915_450_073_139_u64)
    );

    // Through JSON text, so the u64 > i64::MAX value really crosses a parser.
    let text = serde_json::to_string(&wire).expect("to text");
    let value: Value = serde_json::from_str(&text).expect("from text");
    let mut restored = state_from_json(&value).expect("import");

    assert_eq!(restored.run_rng.seed, 273_692_109);
    assert_eq!(restored.run_rng.player_seed, 15_676_685_915_450_073_139);
    assert_eq!(restored.run_rng.seed_string.as_deref(), Some("oldP0A0"));
    let mut original = original;
    // The player stream's NEXT draw matches the original's.
    assert_eq!(
        restored.run_rng.rewards.next_int(0, 999),
        original.run_rng.rewards.next_int(0, 999)
    );
    assert_eq!(restored.run_rng.rewards.counter(), 4);
    assert_eq!(
        restored.run_rng.shuffle.next_int(0, 999),
        original.run_rng.shuffle.next_int(0, 999)
    );
}

#[test]
fn seeds_above_i64_max_survive_the_wire() {
    let original = state_with_seed("P0A0", 1, 0);
    let wire = combat_state_to_value(&original).expect("emit");
    assert_eq!(
        wire["rng"]["run_seed"],
        json!(10_883_704_133_362_413_376_u64)
    );
    assert_eq!(
        wire["rng"]["player_seed"],
        json!(10_883_704_133_362_413_376_u64)
    );
    let text = serde_json::to_string(&wire).expect("to text");
    let restored = state_from_json(&serde_json::from_str(&text).unwrap()).expect("import");
    assert_eq!(restored.run_rng.seed, 10_883_704_133_362_413_376);
    assert_eq!(restored.run_rng.player_seed, 10_883_704_133_362_413_376);
}

#[test]
fn legacy_u32_wire_restores_run_streams_and_refuses_player_counters() {
    let mut wire = combat_state_to_value(&state_with_seed("oldP0A0", 0, 0)).expect("emit");
    // A pre-0.109 producer: one u32 `run_seed`, no `player_seed`/`seed_string`.
    wire["rng"] = json!({"run_seed": 273_692_109_u64, "counters": {"shuffle": 2}});
    let restored = state_from_json(&wire).expect("run streams restore");
    assert_eq!(restored.run_rng.seed, 273_692_109);
    assert_eq!(restored.run_rng.shuffle.counter(), 2);

    wire["rng"] = json!({"run_seed": 273_692_109_u64, "counters": {"rewards": 1}});
    let err = state_from_json(&wire).expect_err("player stream is unrecoverable");
    let msg = format!("{err}");
    assert!(msg.contains("rewards"), "{msg}");
}

#[test]
fn seed_string_recovers_the_player_base_and_must_agree_with_run_seed() {
    let mut wire = combat_state_to_value(&state_with_seed("oldP0A0", 0, 0)).expect("emit");
    wire["rng"] = json!({
        "run_seed": 273_692_109_u64,
        "seed_string": "oldP0A0",
        "counters": {"rewards": 2}
    });
    let restored = state_from_json(&wire).expect("seed_string path");
    assert_eq!(restored.run_rng.player_seed, 15_676_685_915_450_073_139);
    assert_eq!(restored.run_rng.rewards.counter(), 2);

    wire["rng"] = json!({"run_seed": 1_u64, "seed_string": "oldP0A0"});
    assert!(
        state_from_json(&wire).is_err(),
        "a string that does not hash to run_seed is rejected"
    );
}
