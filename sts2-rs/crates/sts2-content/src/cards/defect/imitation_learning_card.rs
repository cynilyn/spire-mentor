//! Defect Imitation Learning — v0.111.0-only, MP-only (GH #793).
use super::prelude::*;

/// Imitation Learning (Rare, 1 energy, Skill, Exhaust, AnyAlly) — v0.111.0
/// `Cards/ImitationLearning.cs`, `MultiplayerConstraint => MultiplayerOnly`:
///
/// [decompiled C# omitted — see the cited path]
///
/// `AnyAlly` — unplayable in single-player (`card_effects::card_is_any_ally`).
pub(crate) fn play_imitation_learning(
    _state: &mut CombatState,
    _t: Option<CreatureUid>,
) -> EffectResult {
    // TODO(S8b #794): ImitationLearningPower 2 (upg 3) bound to the target
    // player once the power is ported. Unreachable in single-player.
    EffectResult::Resolved
}

#[must_use]
pub(crate) const fn spec() -> CardSpec {
    CardSpec {
        id: CardId::ImitationLearningCard,
        name: "IMITATION_LEARNING_CARD",
        cost: 1,
        card_type: CardType::Skill,
        target_type: TargetType::Self_,
        rarity: CardRarity::Rare,
        base_damage: 0,
        base_block: 0,
        keywords: CardKeywords::EXHAUST,
        ..CardSpec::base()
    }
}

#[allow(clippy::unnecessary_wraps, dead_code)]
pub(crate) fn upgrade() -> Option<CardUpgradeDelta> {
    None
}
