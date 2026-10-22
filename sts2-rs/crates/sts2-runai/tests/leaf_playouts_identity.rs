//! Byte-identity pin for the deployed combat search (GH #830, deep bets
//! build 1).
//!
//! `MctsConfig::leaf_playouts` (default 1) adds extra playouts per search
//! leaf. At its default it must leave the search exactly as it was. This
//! file pins what the deployed search produced on the engine the knob was
//! added to (`2ecc2ba40`), blessed BEFORE the knob existed:
//!
//! - **Searches.** Fight states from fresh Ironclad runs, each searched at
//!   300 sims under the deployed config
//!   ([`MctsCombatPolicy::search_config`]): every root child's visit / win /
//!   loss / tie counts and the bits of its value, squared-value and HP sums,
//!   the chosen action, and the rollout counters.
//! - **A fight.** The first combat of a fresh run, played by
//!   [`MctsCombatPolicy`] at 1000 sims: every action and the final HP.
//!
//! A comparison of `leaf_playouts: 1` against `MctsConfig::default()` could
//! not fail: the default IS 1, so a bug in the `k = 1` path would move both
//! sides at once. The golden is what makes this a test. (The fixture
//! searches were blessed before `leaf_playouts` existed, so their config is
//! "the field not set"; the explicit `leaf_playouts: 1` arm compares against
//! the same pin.)
//!
//! Re-bless only when an engine or content change moves the search on
//! purpose (any card, monster or search change in these fights will), never
//! to make a `leaf_playouts` change pass:
//! `STS2_BLESS_LEAF_PLAYOUTS_GOLDEN=1 cargo nextest run -p sts2-runai --test leaf_playouts_identity`.

use serde_json::{json, Value};
use sts2_core::run_manager::{Phase, RunManager};
use sts2_core::{apply, is_terminal, legal_actions, CombatState};
use sts2_mcts::{LeafEval, Mcts, MctsConfig};
use sts2_runai::policy::{CombatBudget, HeuristicPolicy, MctsCombatPolicy};
use sts2_runai::rollout::ensure_content_installed;

const GOLDEN: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/leaf_playouts_identity_golden.json"
);

/// Search budget of the fixture searches. Small enough for a debug build,
/// large enough that the tree has chance nodes several plies deep.
const FIXTURE_SIMS: u32 = 300;

fn fresh_run(seed: i32) -> RunManager {
    ensure_content_installed();
    let mut rm = sts2_content::run_callbacks::new_run_manager_for_character_with_ascension(
        seed,
        sts2_core::potions::CharacterId::Ironclad,
        0,
    )
    .with_neow_start();
    sts2_content::run_callbacks::initialize_run_setup(&mut rm);
    rm
}

/// Drive `rm` with first-option picks until it enters its first combat.
fn to_first_combat(rm: &mut RunManager) {
    for _ in 0..64 {
        if rm.phase() == Phase::Combat {
            return;
        }
        let actions = rm.get_available_actions();
        assert!(!actions.is_empty(), "run stalled before its first combat");
        let _ = rm.take_action(actions[0]);
    }
    panic!("no combat within 64 run actions");
}

/// Fight states: the opening position of the first combat of three fresh
/// runs; each one a few deterministic (index-hashed) plays later; and each
/// opening with the enemies at 9 HP and the player at 7, so the searches
/// also back up won and lost terminals and proven leaves, not only the
/// depth-200 playouts of a healthy opening.
fn fixture_states() -> Vec<CombatState> {
    let mut out = Vec::new();
    for seed in [3_i32, 7, 11] {
        let mut rm = fresh_run(seed);
        to_first_combat(&mut rm);
        let mut s = rm.combat.clone().expect("in combat");
        out.push(s.clone());
        let mut close = s.clone();
        let player_uid = close.player.creature_uid.map(|u| u.0);
        for c in &mut close.creatures {
            if close.enemy_uids.iter().any(|u| u.0 == c.id) {
                c.current_hp = c.current_hp.min(9);
            } else if Some(c.id) == player_uid {
                c.current_hp = 7; // the player's mirror creature
            }
        }
        close.player.current_hp = 7;
        assert_eq!(
            sts2_core::invariants::check_combat_invariants(&close, "close fixture"),
            Ok(())
        );
        out.push(close);
        let mut k: usize = usize::try_from(seed).expect("positive");
        for _ in 0..4 {
            if is_terminal(&s) {
                break;
            }
            let acts = legal_actions(&s);
            if acts.is_empty() {
                break;
            }
            k = k.wrapping_mul(2_654_435_761).wrapping_add(17);
            let _ = apply(&mut s, acts[k % acts.len()].clone());
        }
        if !is_terminal(&s) {
            out.push(s);
        }
    }
    out
}

/// The deployed search's config for decision `seed` of a 300-sim search,
/// with `edit` applied on top.
fn deployed(seed: u64, edit: impl Fn(&mut MctsConfig)) -> (MctsConfig, LeafEval) {
    let policy = MctsCombatPolicy::new(HeuristicPolicy::new(), CombatBudget::eval_cheap(), 0);
    let (mut cfg, leaf) = policy.search_config(FIXTURE_SIMS, seed);
    edit(&mut cfg);
    (cfg, leaf)
}

fn search_record(state: &CombatState, cfg: MctsConfig, leaf: LeafEval) -> Value {
    let mut mcts = Mcts::new(state.clone(), cfg).with_leaf_eval(leaf);
    let chosen = mcts.try_search_n(FIXTURE_SIMS).expect("non-terminal root");
    let (rollouts, cap_hits) = mcts.rollout_stats();
    let root: Vec<Value> = mcts
        .root_outcomes()
        .iter()
        .map(|o| {
            json!([
                format!("{:?}", o.action),
                o.visits,
                o.wins,
                o.losses,
                o.ties,
                o.value_sum.to_bits(),
                o.value_sq_sum.to_bits(),
                o.hp_sum.to_bits(),
                o.hp_n,
            ])
        })
        .collect();
    json!({
        "chosen": format!("{chosen:?}"),
        "rollouts_total": rollouts,
        "depth_cap_hits": cap_hits,
        "rollouts_skipped": mcts.rollouts_skipped(),
        "root": root,
    })
}

fn searches(edit: impl Fn(&mut MctsConfig) + Copy) -> Value {
    let states = fixture_states();
    assert!(states.len() >= 5, "{} fixture states", states.len());
    Value::Array(
        states
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let seed = 0x5EED_0000 + u64::try_from(i).expect("small");
                let (cfg, leaf) = deployed(seed, edit);
                search_record(s, cfg, leaf)
            })
            .collect(),
    )
}

/// The first combat of a fresh seed-5 run, played by the deployed
/// `MctsCombatPolicy` at 1000 sims (its non-combat picks come from the
/// heuristic meta).
fn policy_fight() -> Value {
    let mut rm = fresh_run(5);
    let mut policy = MctsCombatPolicy::new(HeuristicPolicy::new(), CombatBudget::eval_cheap(), 5);
    let mut combat_actions = Vec::new();
    let mut entered = false;
    for _ in 0..2_000 {
        let in_combat = rm.phase() == Phase::Combat;
        if entered && !in_combat {
            break;
        }
        entered |= in_combat;
        let actions = rm.get_available_actions();
        assert!(!actions.is_empty(), "run stalled");
        let idx = sts2_core::full_run::RunPolicy::choose(&mut policy, &rm, &actions)
            .expect("policy picks");
        if in_combat {
            combat_actions.push(format!("{:?}", actions[idx]));
        }
        let _ = rm.take_action(actions[idx]);
    }
    assert!(entered, "never entered combat");
    assert!(rm.phase() != Phase::Combat, "fight did not finish");
    let (elite, boss, motif, sims) = policy.combat_budget_spent();
    json!({
        "actions": combat_actions,
        "player_hp": rm.run_state.current_hp,
        "budget_spent": [elite, boss, motif, sims],
    })
}

fn golden() -> Value {
    serde_json::from_str(&std::fs::read_to_string(GOLDEN).expect("golden file")).expect("json")
}

fn bless_or_compare(key: &str, got: &Value) {
    if std::env::var_os("STS2_BLESS_LEAF_PLAYOUTS_GOLDEN").is_some() {
        let mut all = std::fs::read_to_string(GOLDEN)
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .unwrap_or_else(|| json!({}));
        all[key] = got.clone();
        std::fs::write(
            GOLDEN,
            serde_json::to_string_pretty(&all).expect("json") + "\n",
        )
        .expect("write golden");
        return;
    }
    let want = &golden()[key];
    assert!(!want.is_null(), "golden has no {key:?} entry");
    assert_eq!(got, want, "{key}: deployed search output moved");
}

#[test]
fn deployed_search_output_matches_the_pre_knob_golden() {
    bless_or_compare("searches", &searches(|_| {}));
}

/// The knob set explicitly to its default is the same search, and the
/// golden can see the knob at all: four playouts per leaf move it.
#[test]
fn explicit_one_leaf_playout_matches_the_golden_and_four_do_not() {
    assert_eq!(MctsConfig::default().leaf_playouts, 1);
    bless_or_compare("searches", &searches(|c| c.leaf_playouts = 1));
    if std::env::var_os("STS2_BLESS_LEAF_PLAYOUTS_GOLDEN").is_none() {
        // Two fixtures suffice (and keep a debug build fast): the opening
        // and the close fight of the first run.
        let states = fixture_states();
        for (i, s) in states.iter().take(2).enumerate() {
            let seed = 0x5EED_0000 + u64::try_from(i).expect("small");
            let (cfg, leaf) = deployed(seed, |c| c.leaf_playouts = 4);
            assert_ne!(
                search_record(s, cfg, leaf),
                golden()["searches"][i],
                "fixture {i}: leaf_playouts=4 reproduced the single-playout golden"
            );
        }
    }
}

#[test]
fn deployed_policy_fight_at_1000_sims_matches_the_pre_knob_golden() {
    bless_or_compare("policy_fight", &policy_fight());
}
