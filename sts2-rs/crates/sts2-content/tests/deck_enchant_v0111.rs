//! Deck-enchant picks follow each enchantment's own `CanEnchant` (v0.111.0); v0.107.1 keeps the old pool.

use sts2_core::GameVersion;

/// Every enchantment's own `CanEnchant` restricts the pick, not only Soul's Power's Exhaust gate
/// (v0.111.0): Corrupted / Sharp offer Attacks, Imbued offers Skills.
#[test]
fn enchant_pick_candidates_follow_each_enchantments_can_enchant_on_v0111_only() {
    use sts2_core::cards::{CardType, EnchantmentId};
    use sts2_core::PendingDeckEnchant;
    sts2_content::relics::dispatch::ensure_installed();
    let candidates = |version: GameVersion, ench: EnchantmentId| -> Vec<CardType> {
        let mut rm = sts2_content::run_callbacks::new_run_manager_for_character(
            7,
            sts2_core::potions::CharacterId::Ironclad,
        );
        rm.run_state.game_version = version;
        rm.run_state.pending_deck_enchants.push(PendingDeckEnchant {
            enchantment: ench,
            amount: 1,
            type_restriction: None,
            secondary_type: None,
        });
        rm.enchant_eligible_deck_indices()
            .into_iter()
            .map(|i| rm.run_state.master_deck[i].card_type)
            .collect()
    };
    for (ench, only) in [
        (EnchantmentId::Corrupted, CardType::Attack),
        (EnchantmentId::Sharp, CardType::Attack),
        (EnchantmentId::Imbued, CardType::Skill),
    ] {
        let now = candidates(GameVersion::V0_111_0, ench);
        assert!(
            !now.is_empty() && now.iter().all(|t| *t == only),
            "{ench:?} {now:?}"
        );
        let before = candidates(GameVersion::V0_107_1, ench);
        assert!(
            before.iter().any(|t| *t != only),
            "{ench:?} unchanged on 0.107.1"
        );
    }
}
