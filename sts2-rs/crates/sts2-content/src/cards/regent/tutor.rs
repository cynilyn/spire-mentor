//! Regent Tutor — v0.111.0-only, MP-only (GH #793).
use super::prelude::*;

/// Tutor (Rare, 1 energy, Skill, AnyAlly) — v0.111.0 `Cards/Tutor.cs`,
/// `MultiplayerConstraint => MultiplayerOnly`:
///
/// [decompiled C# omitted — see the cited path]
///
/// `AnyAlly` — C# `CardModel.CanPlay` (`CardModel.cs:1742`) rejects it
/// without a living ally, which single-player never has
/// (`card_effects::card_is_any_ally`), so the play never resolves here and
/// the ally's draw-pile pick is not modelled.
pub(crate) fn play_tutor(_state: &mut CombatState, _t: Option<CreatureUid>) -> EffectResult {
    // v0.111.0: the TARGET ally picks from its own draw pile
    // (decomp Tutor.cs:20-29) — unreachable in single-player.
    EffectResult::Resolved
}

#[must_use]
pub(crate) const fn spec() -> CardSpec {
    CardSpec {
        id: CardId::Tutor,
        name: "TUTOR",
        cost: 1,
        card_type: CardType::Skill,
        target_type: TargetType::Self_,
        rarity: CardRarity::Rare,
        base_damage: 0,
        base_block: 0,
        keywords: CardKeywords::NONE,
        ..CardSpec::base()
    }
}

#[allow(clippy::unnecessary_wraps, dead_code)]
pub(crate) fn upgrade() -> Option<CardUpgradeDelta> {
    // `EnergyCost.UpgradeBy(-1)` — the row lives in `card_upgrades_generated`.
    None
}
