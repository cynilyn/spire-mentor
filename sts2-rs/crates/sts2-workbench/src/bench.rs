//! `/api/bench`: the M1 phone-speed exit criterion.
//!
//! Runs the same single-tree combat search the Python workbench's hint
//! endpoint runs (`sts2_rs.mcts_recommend` with `threads=1`, the config
//! pinned in `sts2-py/src/lib.rs::mcts_recommend`) plus a batch of seeded
//! heuristic playouts (`heuristic_play_to_terminal`) over the LIVE combat
//! state, and reports the work done. The host measures wall-clock around
//! the call — this crate has no clock — so the number recorded in
//! `docs/perf/` is `ms / sim` on the device that ran it.
//!
//! Also the seam the M2 advisors reuse: `recommend` is the hint endpoint's
//! search half.

use rand::SeedableRng;
use serde_json::{json, Value};
use sts2_core::{apply, legal_actions, terminal_value, Action, CombatState};
use sts2_mcts::{HiddenInfoMode, Mcts, MctsConfig, RolloutPolicyKind, SharedPolicy};
use sts2_wire::emit::combat_action_to_value;

/// `mcts_recommend`'s default `c_puct` for a NON-honest mode (`sqrt(2)`).
///
/// The default mode is BeliefDraw, whose constant is a function of the budget
/// — [`sts2_mcts::belief_draw_c_puct`] — so this bench reads it per call
/// rather than pinning a number here.
pub const DEFAULT_C_PUCT: f32 = std::f32::consts::SQRT_2;

/// The single-tree search of `sts2_rs.mcts_recommend(..., threads=1,
/// fair=False)`: root outcomes plus the principal variation.
#[must_use]
pub fn recommend(state: &CombatState, sims: u32, seed: u64) -> Value {
    let legal = legal_actions(state);
    // `sims == 0` is a playouts-only bench; the search is not built at all.
    if sims == 0 || legal.is_empty() {
        return json!({"scores": [], "pv": [], "sims": sims});
    }
    // Mirrors `mcts_recommend`'s pinned config, which is the whole point of
    // this module — including its DEFAULT honest triple (beliefdraw, the
    // budget's `belief_draw_c_puct`, draw widening 2.0). It tracked the old
    // clairvoyant default before; leaving it there would have made the
    // phone-speed number measure a search the app no longer runs.
    let cfg = MctsConfig {
        c_puct: sts2_mcts::belief_draw_c_puct(sims),
        max_simulations: sims,
        max_rollout_depth: 200,
        seed,
        rollout_policy: RolloutPolicyKind::Heuristic,
        use_transposition_table: false,
        regularized_pi_temperature: 0.1,
        use_puct_prior: false,
        hidden_info: HiddenInfoMode::BeliefDraw,
        draw_widening_c: 2.0,
        ..MctsConfig::default()
    };
    let mut m = Mcts::with_policy(state.clone(), cfg, SharedPolicy::uniform());
    let _ = m.search();
    let scores: Vec<Value> = m
        .root_outcomes()
        .into_iter()
        .map(|o| {
            json!({
                "action": combat_action_to_value(&o.action),
                "visits": o.visits,
                "wins": o.wins,
                "losses": o.losses,
                "ties": o.ties,
                "value_sum": f64::from(o.value_sum),
            })
        })
        .collect();
    let pv: Vec<Value> = m
        .get_pv()
        .into_iter()
        .take(8)
        .map(|a| combat_action_to_value(&a))
        .collect();
    json!({"scores": scores, "pv": pv, "sims": sims})
}

/// `heuristic_play_to_terminal`: one seeded heuristic playout.
#[must_use]
pub fn play_heuristic_to_terminal(
    state: &CombatState,
    seed: u64,
    max_actions: u32,
) -> (bool, i32, u32) {
    let mut s = state.clone();
    let mut rng = rand_xoshiro::Xoshiro256PlusPlus::seed_from_u64(seed);
    let mut actions = 0_u32;
    while !terminal_value(&s).0 && actions < max_actions {
        let legal = legal_actions(&s);
        if legal.is_empty() {
            let _ = apply(&mut s, Action::EndTurn);
            actions += 1;
            continue;
        }
        let idx = sts2_mcts::expert::pick_action_heuristic(&mut rng, &s, &legal);
        let _ = apply(&mut s, legal[idx].clone());
        actions += 1;
    }
    (s.player_won, s.player.current_hp, actions)
}

/// The bench body: `sims` simulations of search, then `playouts` heuristic
/// playouts, over `state`.
#[must_use]
pub fn run(state: &CombatState, sims: u32, playouts: u32, seed: u64) -> Value {
    let search = recommend(state, sims, seed);
    let mut wins = 0_u32;
    let mut actions_total = 0_u64;
    for i in 0..playouts {
        let (won, _hp, actions) =
            play_heuristic_to_terminal(state, seed.wrapping_add(u64::from(i)), 2000);
        if won {
            wins += 1;
        }
        actions_total += u64::from(actions);
    }
    json!({
        "sims": sims,
        "best_action": search["scores"].as_array().and_then(|s| s.first()).map(|s| s["action"].clone()),
        "root_actions": search["scores"].as_array().map_or(0, Vec::len),
        "pv": search["pv"],
        "playouts": {"n": playouts, "wins": wins, "actions": actions_total},
    })
}
