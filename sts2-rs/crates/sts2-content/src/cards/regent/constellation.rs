//! Regent Constellation — v0.111.0-only, MP-only (GH #793).
use super::prelude::*;

/// Constellation (Uncommon, 0 energy + 2 stars, Skill, AnyAlly) — v0.111.0
/// `Cards/Constellation.cs`, `MultiplayerConstraint => MultiplayerOnly`:
///
/// [decompiled C# omitted — see the cited path]
///
/// `AnyAlly` — unplayable in single-player (`card_effects::card_is_any_ally`);
/// the body below is the target-collapsed effect for completeness.
pub(crate) fn play_constellation(state: &mut CombatState, _t: Option<CreatureUid>) -> EffectResult {
    // v0.111.0: target draws 1, gains 1 energy and 9 (upg 12) Block
    // (decomp Constellation.cs:31-38).
    let _ = sts2_core::cards::draw_via_state(state, 1);
    state.gain_energy(1);
    let _ = gain_block_player(state, current_block(state));
    EffectResult::Resolved
}

#[must_use]
pub(crate) const fn spec() -> CardSpec {
    CardSpec {
        id: CardId::Constellation,
        name: "CONSTELLATION",
        cost: 0,
        card_type: CardType::Skill,
        target_type: TargetType::Self_,
        rarity: CardRarity::Uncommon,
        base_damage: 0,
        base_block: 9,
        keywords: CardKeywords::NONE,
        ..CardSpec::base()
    }
}

#[allow(clippy::unnecessary_wraps, dead_code)]
pub(crate) fn upgrade() -> Option<CardUpgradeDelta> {
    // Block 9 -> 12 — the row lives in `card_upgrades_generated`.
    None
}
