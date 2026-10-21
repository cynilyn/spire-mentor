//! A card an event adds to the deck goes through the deck-add hooks (v0.111.0).
//!
//! C# `CardPileCmd.Add(card, Deck)` runs `ModifyCardBeingAddedToDeck` (the eggs) and the
//! `AfterCardChangedPiles` listeners (Lucky Fysh's 15 gold); the event handlers used to push
//! onto the deck directly, skipping both.

use sts2_core::events::EventId;
use sts2_core::relics::RelicId;
use sts2_core::runstate::RunState;
use sts2_core::GameVersion;

fn gold_after_bugslayer(version: GameVersion) -> i32 {
    sts2_content::relics::dispatch::ensure_installed();
    let mut rs = RunState::new();
    rs.game_version = version;
    rs.gold = 100;
    rs.relics.push(RelicId::LuckyFysh);
    rs.enter_event_room(EventId::Bugslayer);
    let deck = rs.master_deck.len();
    let _ = sts2_content::events::dispatch::resolve(&mut rs, "exterminate").expect("event active");
    assert_eq!(rs.master_deck.len(), deck + 1);
    rs.gold - 100
}

#[test]
fn bugslayer_card_pays_lucky_fysh_on_v0111_only() {
    assert_eq!(gold_after_bugslayer(GameVersion::V0_111_0), 15);
    assert_eq!(gold_after_bugslayer(GameVersion::V0_107_1), 0);
}

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
