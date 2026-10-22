//! Turn-plan enumerator gates (GH #830, turn-plan G0 Task 1).
//!
//! `enumerate_plans` lists every deterministic action sequence of the
//! player's turn — up to `EndTurn`, the first RNG-consuming action, or a
//! terminal — deduped by resulting state (`tt_hash_v2`).

mod common;

use std::collections::HashSet;

use common::{fixture, push_draw};
use smallvec::SmallVec;
use sts2_core::cards::CardId;
use sts2_core::potions::PotionKind;
use sts2_core::{apply, is_terminal, legal_actions, tt_hash_v2, Action, CombatState, CreatureUid};
use sts2_mcts::turn_plan::{enumerate_plans, PlanCap, PlanEnd, TurnPlan};
use sts2_mcts::HiddenInfoMode;

/// Add a second enemy, a copy of the first with the next creature uid.
fn add_enemy(state: &mut CombatState, hp: i32) {
    let mut second = state.creatures[0].clone();
    let uid = CreatureUid::new(2).expect("non-zero");
    second.id = uid.0;
    second.current_hp = hp;
    second.max_hp = hp;
    state.creatures.push(second);
    state.enemy_uids.push(uid);
}

/// The card id each action plays (`None` for non-card actions), replayed
/// on a clone of `state`; also returns the state before the last action.
fn replay(state: &CombatState, plan: &TurnPlan) -> (Vec<Option<CardId>>, CombatState) {
    let mut s = state.clone();
    let mut ids = Vec::new();
    let mut before_last = s.clone();
    for a in &plan.actions {
        before_last = s.clone();
        let id = match a {
            Action::PlayCard { hand_index, .. } => {
                let uid = s.player.hand[usize::from(*hand_index)];
                Some(s.cards[uid.0.get() as usize - 1].card_id)
            }
            _ => None,
        };
        ids.push(id);
        let _ = apply(&mut s, a.clone());
    }
    (ids, before_last)
}

/// Raw (undeduped) legal action sequences up to EndTurn or terminal.
fn raw_sequences(state: &CombatState) -> usize {
    let mut n = 0;
    for a in legal_actions(state) {
        if a == Action::EndTurn {
            n += 1;
            continue;
        }
        let mut s = state.clone();
        let _ = apply(&mut s, a);
        if is_terminal(&s) {
            n += 1;
        } else {
            n += raw_sequences(&s);
        }
    }
    n
}

#[test]
fn empty_hand_has_only_end_turn() {
    let state = fixture(&[], 40, &[], 3);
    let set = enumerate_plans(&state, HiddenInfoMode::BeliefDraw, PlanCap::default());
    assert_eq!(
        set.plans,
        vec![TurnPlan {
            actions: vec![Action::EndTurn],
            end: PlanEnd::EndTurn,
        }]
    );
    assert!(!set.cap_hit);
}

#[test]
fn strike_strike_bash_dedups_to_end_states() {
    let state = fixture(
        &[CardId::StrikeIronclad, CardId::StrikeIronclad, CardId::Bash],
        40,
        &[],
        3,
    );
    let set = enumerate_plans(&state, HiddenInfoMode::BeliefDraw, PlanCap::default());
    assert!(!set.cap_hit);

    let mut pre_end_hashes = HashSet::new();
    let mut seqs = Vec::new();
    for plan in &set.plans {
        assert_eq!(plan.end, PlanEnd::EndTurn, "no randomness here: {plan:?}");
        assert_eq!(plan.actions.last(), Some(&Action::EndTurn));
        let (ids, pre_end) = replay(&state, plan);
        assert!(
            pre_end_hashes.insert(tt_hash_v2(&pre_end)),
            "two EndTurn plans share a pre-EndTurn state: {plan:?}"
        );
        seqs.push(ids);
    }

    let s = Some(CardId::StrikeIronclad);
    let b = Some(CardId::Bash);
    let count = |want: &[Option<CardId>]| seqs.iter().filter(|q| q.as_slice() == want).count();
    assert_eq!(
        count(&[s, s, None]),
        1,
        "Strike copies are one identity: {seqs:?}"
    );
    assert_eq!(
        count(&[b, s, None]),
        1,
        "Bash-then-Strike missing: {seqs:?}"
    );
    assert_eq!(
        count(&[s, b, None]),
        1,
        "Strike-then-Bash missing: {seqs:?}"
    );
    assert!(
        set.plans.len() < raw_sequences(&state),
        "dedup removed nothing: {} plans",
        set.plans.len()
    );
}

#[test]
fn random_target_card_ends_plan() {
    let mut state = fixture(
        &[CardId::SwordBoomerang, CardId::StrikeIronclad],
        40,
        &[],
        3,
    );
    add_enemy(&mut state, 40);
    let set = enumerate_plans(&state, HiddenInfoMode::BeliefDraw, PlanCap::default());
    let mut with_boomerang = 0;
    for plan in &set.plans {
        let (ids, _) = replay(&state, plan);
        if let Some(pos) = ids.iter().position(|&i| i == Some(CardId::SwordBoomerang)) {
            with_boomerang += 1;
            assert_eq!(pos, plan.actions.len() - 1, "boomerang not last: {plan:?}");
            assert_eq!(plan.end, PlanEnd::Chance, "{plan:?}");
        }
    }
    assert!(with_boomerang > 0, "no plan plays SwordBoomerang: {set:?}");
}

#[test]
fn draw_ends_plan_under_belief_draw() {
    let mut state = fixture(&[CardId::PommelStrike], 40, &[], 3);
    push_draw(
        &mut state,
        &[CardId::StrikeIronclad, CardId::DefendIronclad],
    );
    let set = enumerate_plans(&state, HiddenInfoMode::BeliefDraw, PlanCap::default());
    let pommel: Vec<_> = set
        .plans
        .iter()
        .filter(|p| matches!(p.actions.first(), Some(Action::PlayCard { .. })))
        .collect();
    assert_eq!(pommel.len(), 1, "{set:?}");
    assert_eq!(pommel[0].actions.len(), 1, "{set:?}");
    assert_eq!(pommel[0].end, PlanEnd::Chance);
}

#[test]
fn potion_plan_is_distinct_from_no_potion_plan() {
    let state = fixture(&[], 40, &[PotionKind::BlockPotion], 3);
    let set = enumerate_plans(&state, HiddenInfoMode::BeliefDraw, PlanCap::default());
    assert_eq!(set.plans.len(), 2, "{set:?}");
    assert!(set.plans.iter().all(|p| p.end == PlanEnd::EndTurn));
    let lens: HashSet<usize> = set.plans.iter().map(|p| p.actions.len()).collect();
    assert_eq!(lens, HashSet::from([1, 2]));
    let drink = set.plans.iter().find(|p| p.actions.len() == 2).unwrap();
    assert!(matches!(drink.actions[0], Action::DrinkPotion { .. }));
}

#[test]
fn lethal_prefix_ends_terminal() {
    let state = fixture(&[CardId::StrikeIronclad], 6, &[], 3);
    let set = enumerate_plans(&state, HiddenInfoMode::BeliefDraw, PlanCap::default());
    let lethal: Vec<_> = set
        .plans
        .iter()
        .filter(|p| matches!(p.actions.first(), Some(Action::PlayCard { .. })))
        .collect();
    assert_eq!(lethal.len(), 1, "{set:?}");
    assert_eq!(lethal[0].actions.len(), 1, "no EndTurn after lethal");
    assert_eq!(lethal[0].end, PlanEnd::Terminal);
}

#[test]
fn cap_hit_reported() {
    let state = fixture(
        &[CardId::StrikeIronclad, CardId::StrikeIronclad, CardId::Bash],
        40,
        &[],
        3,
    );
    let cap = PlanCap {
        max_plans: 3,
        max_nodes: 20_000,
    };
    let set = enumerate_plans(&state, HiddenInfoMode::BeliefDraw, cap);
    assert!(set.cap_hit);
    assert!(set.plans.len() <= 3);
}

#[test]
fn every_plan_replays_legally() {
    let state = fixture(
        &[
            CardId::StrikeIronclad,
            CardId::DefendIronclad,
            CardId::Bash,
            CardId::Anger,
            CardId::Inflame,
        ],
        60,
        &[PotionKind::BlockPotion],
        3,
    );
    let set = enumerate_plans(&state, HiddenInfoMode::BeliefDraw, PlanCap::default());
    assert!(!set.cap_hit);
    assert!(set.plans.len() > 5, "{} plans", set.plans.len());
    for plan in &set.plans {
        let mut s = state.clone();
        for a in &plan.actions {
            let legal: SmallVec<[Action; 16]> = legal_actions(&s);
            assert!(legal.contains(a), "{a:?} illegal in plan {plan:?}");
            let _ = apply(&mut s, a.clone());
        }
    }
}
