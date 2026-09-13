// port-pin-skip — DSL re-export glue (sts2-core monster_ai is canonical).
pub use sts2_core::monster_ai::{
    ConditionFn, ConditionalBranch, ConditionalBranchState, EffectFn, MonsterAi, MonsterState,
    MoveRepeatType, MoveState, RandomBranchState, StateId, WeightedBranch,
};

use sts2_core::monsters::Intent;
use sts2_core::{CombatState, CreatureUid};

/// Shared no-op [`EffectFn`] for telegraph-only moves whose behavior is
/// fully described by their intents.
pub(crate) fn noop_effect(_: &mut CombatState, _: CreatureUid) {}

/// Move-state builder: a telegraph-only move (no effect beyond its
/// intents) that transitions to `follow_up`.
pub(crate) fn mv(
    id: &'static str,
    intents: smallvec::SmallVec<[Intent; 4]>,
    follow_up: &'static str,
) -> MonsterState {
    MonsterState::Move(MoveState {
        id,
        effect: noop_effect,
        intents,
        follow_up: Some(follow_up),
        must_perform_once: false,
    })
}

/// [`mv`] variant that attaches a non-noop `effect_fn` (bead
/// `sts2-rs-vn0m`). Used by summoner moves (e.g. Fogmog ILLUSION, Vantom
/// CONSUME) whose effect must actually call `combat.spawn_enemy`, and by
/// telegraph-only Buff/Debuff moves that must apply a real power (bead
/// `sts2-rs-pdsdl`).
pub(crate) fn mv_eff(
    id: &'static str,
    intents: smallvec::SmallVec<[Intent; 4]>,
    follow_up: &'static str,
    effect: EffectFn,
) -> MonsterState {
    MonsterState::Move(MoveState {
        id,
        effect,
        intents,
        follow_up: Some(follow_up),
        must_perform_once: false,
    })
}

/// Weighted-branch builder for [`RandomBranchState`] (no cooldown).
pub(crate) fn br(
    state_id: &'static str,
    repeat: MoveRepeatType,
    max_times: u8,
    weight: f64,
) -> WeightedBranch {
    WeightedBranch {
        state_id,
        repeat_type: repeat,
        max_times,
        weight,
        cooldown: 0,
    }
}

/// Stateless [`ConditionalBranch`] builder for [`ConditionalBranchState`].
pub(crate) fn cb(condition: fn(&CombatState) -> bool, state_id: &'static str) -> ConditionalBranch {
    ConditionalBranch {
        condition: ConditionFn::Stateless(condition),
        state_id,
    }
}

/// Owner-aware [`ConditionalBranch`] builder — the predicate sees the creature
/// whose machine is branching. Use whenever the C# condition reads
/// `base.Creature` (`HasPower<…>`, HP thresholds), rather than reaching for a
/// combat-global proxy that cannot see per-creature state.
pub(crate) fn cb_owner(
    condition: fn(&CombatState, sts2_core::CreatureUid) -> bool,
    state_id: &'static str,
) -> ConditionalBranch {
    ConditionalBranch {
        condition: ConditionFn::OwnerAware(condition),
        state_id,
    }
}

/// Catch-all condition for the final [`cb`] fallback branch.
pub(crate) fn always_true(_: &CombatState) -> bool {
    true
}
