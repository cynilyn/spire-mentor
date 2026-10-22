//! Turn-plan enumeration (GH #830, turn-plan search G0).
//!
//! A *plan* is a deterministic action sequence of the player's current
//! turn: it runs until `EndTurn`, until the first action that consumes
//! combat randomness (a draw under `BeliefDraw`, a random target, a roll),
//! or until the combat ends. [`enumerate_plans`] lists every such plan,
//! deduped by the state they reach, so a later stage can price each one.
//!
//! Randomness is detected with the same predicate MCTS expansion uses to
//! allocate a chance node (`apply_detect_stochastic`), and the
//! root gets the same hidden-info conversion the search applies
//! (`apply_hidden_info_root`), so a plan boundary here is a
//! chance node there.

use std::collections::HashSet;

use sts2_core::{is_terminal, legal_actions, tt_hash_v2, Action, CombatState};

use crate::{
    apply_detect_stochastic, apply_hidden_info_root, canonicalize_actions, HiddenInfoMode,
};

/// Fixed seed for the root hidden-info conversion. Plans only depend on
/// which transitions are random, never on how the reseeded streams roll.
pub(crate) const TURN_PLAN_ROOT_SEED: u64 = 0;

/// How a [`TurnPlan`] ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanEnd {
    /// The last action is `EndTurn` (never applied by the enumerator).
    EndTurn,
    /// The last action consumed combat randomness.
    Chance,
    /// The last action ended the combat (a kill or a death).
    Terminal,
}

/// One deterministic action sequence of the current turn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurnPlan {
    /// Actions in order; hand indices are valid against the state each
    /// action is applied to, starting from the enumerated state.
    pub actions: Vec<Action>,
    pub end: PlanEnd,
}

/// Budget for [`enumerate_plans`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlanCap {
    /// Maximum number of plans returned.
    pub max_plans: usize,
    /// Maximum number of non-`EndTurn` actions applied during the search.
    pub max_nodes: usize,
}

impl Default for PlanCap {
    fn default() -> Self {
        Self {
            max_plans: 512,
            max_nodes: 20_000,
        }
    }
}

/// The plans of one turn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanSet {
    pub plans: Vec<TurnPlan>,
    /// A cap stopped the enumeration; `plans` is a prefix of the full set.
    pub cap_hit: bool,
}

/// Enumerate the deterministic plans of `state`'s current turn.
///
/// Depth-first over `canonicalize_actions` order (so identical hand
/// copies are one action and the output order is deterministic). A
/// reached non-chance state (terminal or not) whose `tt_hash_v2` was
/// already seen is skipped, so every `EndTurn` plan ends from a distinct
/// state and every `Terminal` plan reaches a distinct state. `Chance`
/// plans are deduped on `(tt_hash_v2(pre-action state), action)`.
#[must_use]
pub fn enumerate_plans(state: &CombatState, hidden_info: HiddenInfoMode, cap: PlanCap) -> PlanSet {
    let mut root = state.clone();
    apply_hidden_info_root(&mut root, hidden_info, TURN_PLAN_ROOT_SEED);
    let mut e = Enumerator {
        hidden_info,
        cap,
        plans: Vec::new(),
        seen: HashSet::new(),
        chance_seen: HashSet::new(),
        nodes: 0,
        cap_hit: false,
    };
    e.seen.insert(tt_hash_v2(&root));
    e.dfs(&root, &mut Vec::new());
    PlanSet {
        plans: e.plans,
        cap_hit: e.cap_hit,
    }
}

struct Enumerator {
    hidden_info: HiddenInfoMode,
    cap: PlanCap,
    plans: Vec<TurnPlan>,
    seen: HashSet<u64>,
    /// `(tt_hash_v2(pre-action state), action)`; `Action` is not `Hash`,
    /// so the action is keyed by its `Debug` rendering.
    chance_seen: HashSet<(u64, String)>,
    nodes: usize,
    cap_hit: bool,
}

impl Enumerator {
    fn emit(&mut self, prefix: &[Action], last: Action, end: PlanEnd) {
        if self.plans.len() >= self.cap.max_plans {
            self.cap_hit = true;
            return;
        }
        let mut actions = Vec::with_capacity(prefix.len() + 1);
        actions.extend_from_slice(prefix);
        actions.push(last);
        self.plans.push(TurnPlan { actions, end });
    }

    fn dfs(&mut self, state: &CombatState, prefix: &mut Vec<Action>) {
        let acts = canonicalize_actions(state, legal_actions(state));
        let mut state_hash = None;
        for action in acts {
            if self.cap_hit {
                return;
            }
            if action == Action::EndTurn {
                self.emit(prefix, action, PlanEnd::EndTurn);
                continue;
            }
            if self.nodes >= self.cap.max_nodes {
                self.cap_hit = true;
                return;
            }
            self.nodes += 1;
            let mut work = state.clone();
            if apply_detect_stochastic(&mut work, action.clone(), self.hidden_info) {
                let pre = *state_hash.get_or_insert_with(|| tt_hash_v2(state));
                if self.chance_seen.insert((pre, format!("{action:?}"))) {
                    self.emit(prefix, action, PlanEnd::Chance);
                }
                continue;
            }
            if !self.seen.insert(tt_hash_v2(&work)) {
                continue;
            }
            if is_terminal(&work) {
                self.emit(prefix, action, PlanEnd::Terminal);
                continue;
            }
            prefix.push(action);
            self.dfs(&work, prefix);
            prefix.pop();
        }
    }
}
