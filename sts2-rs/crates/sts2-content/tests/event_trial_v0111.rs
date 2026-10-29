//! Trial (`Trial.cs`) is a three-page event on v0.111.0; v0.107.1 keeps its single flavour step.

use sts2_core::events::EventId;
use sts2_core::relics::RelicId;
use sts2_core::runstate::RunState;
use sts2_core::GameVersion;

/// Trial is a three-page event on v0.111.0: Accept rolls the Merchant / Noble / Nondescript page and
/// leaves the event open for Guilty / Innocent (a single flavour step before).
#[test]
fn trial_accept_opens_a_judgment_page_on_v0111_only() {
    use sts2_content::events::dispatch::{present, resolve};
    sts2_content::relics::dispatch::ensure_installed();
    let mut pages = std::collections::BTreeSet::new();
    for seed in 1..40u64 {
        let mut rs = RunState::new();
        rs.game_version = GameVersion::V0_111_0;
        rs.rng = sts2_core::run_rng_set::RunRngSet::new_for(GameVersion::V0_111_0, seed);
        rs.enter_event_room(EventId::Trial);
        let out = resolve(&mut rs, "accept").expect("event active");
        assert!(!out.finished);
        pages.insert(rs.event_state.page);
        let opts = present(&rs).expect("still in the event");
        let ids: Vec<_> = opts.iter().map(|o| o.option_id).collect();
        assert_eq!(ids, ["guilty", "innocent"]);
        let _ = resolve(&mut rs, "guilty").expect("event active");
        assert!(rs.current_event.is_none());
    }
    assert_eq!(pages.len(), 3, "all three judgments reachable: {pages:?}");
    let mut old = RunState::new();
    old.game_version = GameVersion::V0_107_1;
    old.enter_event_room(EventId::Trial);
    assert!(resolve(&mut old, "accept").expect("event active").finished);
}

/// The Nondescript judgment's two card rewards are populated up front (`OfferCustom`) and draw no upgrade
/// float (`ForNonCombatWithDefaultOdds` sets `NoUpgradeRoll`): 6 draws each, 12 in all (v0.111.0).
#[test]
fn trial_nondescript_guilty_rolls_both_card_rewards_without_upgrade_floats() {
    use sts2_content::events::dispatch::resolve;
    sts2_content::relics::dispatch::ensure_installed();
    let mut seen = false;
    for seed in 1..60u64 {
        let mut rs = RunState::new();
        rs.game_version = GameVersion::V0_111_0;
        rs.rng = sts2_core::run_rng_set::RunRngSet::new_for(GameVersion::V0_111_0, seed);
        rs.enter_event_room(EventId::Trial);
        let _ = resolve(&mut rs, "accept").expect("event active");
        let before = rs.rng.rewards.counter();
        let _ = resolve(&mut rs, "guilty").expect("event active");
        if rs.pending_rewards.len() != 2 {
            continue;
        }
        seen = true;
        assert_eq!(rs.rng.rewards.counter() - before, 12, "seed {seed}");
        for r in &rs.pending_rewards {
            let sts2_core::Reward::Card(c) = r else {
                panic!("card reward expected");
            };
            assert_eq!(c.options.len(), 3, "seed {seed}");
        }
    }
    assert!(seen, "the Nondescript page is reachable");
}

/// Rejecting the trial opens the Accept / Double Down page on v0.111.0; Double Down abandons the run
/// (HP 0, which the run manager turns into a defeat). The older build finishes at once on Reject.
#[test]
fn trial_double_down_abandons_the_run_on_v0111() {
    use sts2_content::events::dispatch::{present, resolve};
    sts2_content::relics::dispatch::ensure_installed();
    for seed in 1..6u64 {
        for rod in [false, true] {
            let mut rs = RunState::new();
            rs.game_version = GameVersion::V0_111_0;
            rs.rng = sts2_core::run_rng_set::RunRngSet::new_for(GameVersion::V0_111_0, seed);
            if rod {
                rs.relics.push(RelicId::TungstenRod);
            }
            rs.enter_event_room(EventId::Trial);
            let out = resolve(&mut rs, "reject").expect("event active");
            assert!(!out.finished);
            let opts = present(&rs).expect("still in the event");
            let ids: Vec<_> = opts.iter().map(|o| o.option_id).collect();
            assert_eq!(ids, ["accept", "double_down"]);
            let out = resolve(&mut rs, "double_down").expect("event active");
            assert!(out.finished);
            assert_eq!(rs.current_hp, 0, "seed {seed} rod {rod}");
        }
    }
    let mut old = RunState::new();
    old.game_version = GameVersion::V0_107_1;
    old.enter_event_room(EventId::Trial);
    assert!(resolve(&mut old, "reject").expect("event active").finished);
}
