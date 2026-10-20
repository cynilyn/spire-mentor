//! Ironclad Blaze — v0.111.0-only, MP-only (GH #793).
use super::prelude::*;

/// Blaze (Uncommon, 2 energy, Skill, AnyAlly) — v0.111.0 `Cards/Blaze.cs`,
/// `MultiplayerConstraint => MultiplayerOnly`:
///
/// [decompiled C# omitted — see the cited path]
///
/// `AnyAlly` — unplayable in single-player (`card_effects::card_is_any_ally`);
/// the body below is the target-collapsed effect for completeness.
pub(crate) fn play_blaze(state: &mut CombatState, _t: Option<CreatureUid>) -> EffectResult {
    // v0.111.0: Strength 5 (upg 7) to the target ally (decomp Blaze.cs:24-28).
    let amt = if current_upgraded(state) { 7 } else { 5 };
    apply_power_to_player(state, PowerKind::Strength, amt);
    EffectResult::Resolved
}

#[must_use]
pub(crate) const fn spec() -> CardSpec {
    CardSpec {
        id: CardId::Blaze,
        name: "BLAZE",
        cost: 2,
        card_type: CardType::Skill,
        target_type: TargetType::Self_,
        rarity: CardRarity::Uncommon,
        base_damage: 0,
        base_block: 0,
        keywords: CardKeywords::NONE,
        ..CardSpec::base()
    }
}

#[allow(clippy::unnecessary_wraps, dead_code)]
pub(crate) fn upgrade() -> Option<CardUpgradeDelta> {
    None
}
