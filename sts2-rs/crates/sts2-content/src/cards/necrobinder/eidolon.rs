//! Necrobinder Eidolon. Hand-authored C#-first port.
use super::prelude::*;

/// Eidolon (Rare, 2 energy, Skill, Self). Faithful to C#
/// `Eidolon.OnPlay`: exhaust EVERY card in hand (firing exhaust synergies via
/// `CardCmd.Exhaust`), counting how many were exhausted; if at least 9 were
/// exhausted, gain 1 Intangible. The Intangible threshold (9) is a fixed
/// constant and is NOT affected by upgrade — `OnUpgrade` only lowers the energy
/// cost by 1.
///
/// NOTE: Eidolon exhausts your HAND, but the card itself is NOT Exhaust — the
/// C# `Eidolon` ctor passes no `CardKeyword.Exhaust` (the `ExtraHoverTips`
/// Exhaust tip just documents the hand-exhaust effect), so after resolving the
/// card routes to DISCARD, not Exhaust (Necrobinder sweep `PILE_COUNT|EIDOLON`:
/// C# discard `[…, EIDOLON]` vs an earlier Rust port that self-exhausted it).
// C# source: ../sts2-wiki/decompiled/v0.103.2/MegaCrit.Sts2.Core.Models.Cards/Eidolon.cs
pub(crate) fn play_eidolon(state: &mut CombatState, _target: Option<CreatureUid>) -> EffectResult {
    // Snapshot the hand, then exhaust each card. The Eidolon card itself is on
    // the Play pile (not in hand), so the snapshot is the exact set the C#
    // iterates. `exhaust_card_uid` fires `after_card_exhausted` on the power +
    // relic buses, matching C# `CardCmd.Exhaust`.
    let hand: Vec<sts2_core::CardUid> = state.player.hand.to_vec();
    let mut exhausted_count: i32 = 0;
    for uid in hand {
        if sts2_core::card_effects::exhaust_card_uid(state, uid) {
            exhausted_count += 1;
        }
    }
    if exhausted_count >= 9 {
        apply_power_to_player(state, PowerKind::Intangible, 1);
    }
    EffectResult::Resolved
}

#[must_use]
pub(crate) const fn spec() -> CardSpec {
    CardSpec {
        id: CardId::Eidolon,
        name: "EIDOLON",
        cost: 2,
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
    // C# OnUpgrade: EnergyCost.UpgradeBy(-1). Threshold (9) is constant.
    Some(CardUpgradeDelta {
        cost: Some(1),
        base_damage: None,
        base_block: None,
        keywords: None,
    })
}
