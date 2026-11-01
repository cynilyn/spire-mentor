//! A relic bought from the merchant runs its `AfterObtained` screens right in the shop (v0.111.0):
//! C# `MerchantRelicEntry.OnTryPurchase` awaits `RelicCmd.Obtain`, so Orrery's card rewards,
//! Cauldron's potions and the deck selects of DollysMirror / Kifuda / GnarledHammer / PunchDagger /
//! RoyalStamp open before the player is back at the merchant.

use sts2_core::cards::EnchantmentId;
use sts2_core::potions::CharacterId;
use sts2_core::run_manager::{ActionResult, Phase, RunAction, RunManager};
use sts2_core::shop::{ShopInventory, ShopRelicEntry, ShopRelicRarity};
use sts2_core::{GameVersion, RelicId};

const PRICE: i32 = 150;

/// An interactive host (the adapter's flags) standing in a shop that stocks only `relic`.
fn shop_with(version: GameVersion, relic: RelicId) -> RunManager {
    sts2_content::relics::dispatch::ensure_installed();
    let mut rm = sts2_content::run_callbacks::new_run_manager_for_character_from_string_seed_for(
        version,
        "SHOPRELIC",
        CharacterId::Ironclad,
    );
    rm.run_state.interactive_card_removal = true;
    rm.run_state.interactive_card_upgrade = true;
    rm.run_state.interactive_card_transform = true;
    rm.run_state.direct_event_relic_grant = true;
    rm.run_state.gold = 999;
    let mut inventory = ShopInventory::empty();
    inventory.relics.push(ShopRelicEntry {
        relic_rarity: ShopRelicRarity::Shop,
        relic_id: Some(relic),
        price: PRICE,
    });
    rm.debug_open_shop(inventory);
    rm
}

fn buy(rm: &mut RunManager) {
    assert_eq!(
        rm.take_action(RunAction::ShopBuyRelic { idx: 0 }),
        ActionResult::Advanced
    );
    // TheCourier's discount lowers the debit; nothing else does here.
    assert!(rm.run_state.gold >= 999 - PRICE && rm.run_state.gold < 999);
}

/// The same shop on a non-interactive host (runai / MCTS / parity sweeps).
fn non_interactive(mut rm: RunManager) -> RunManager {
    rm.run_state.interactive_card_removal = false;
    rm.run_state.interactive_card_upgrade = false;
    rm.run_state.interactive_card_transform = false;
    rm.run_state.direct_event_relic_grant = false;
    rm
}

fn enchanted(rm: &RunManager, ench: EnchantmentId) -> usize {
    rm.run_state
        .master_deck
        .iter()
        .filter(|c| c.enchantment(ench) > 0)
        .count()
}

#[test]
fn orrery_offers_five_card_rewards_then_returns_to_the_shop() {
    let mut rm = shop_with(GameVersion::V0_111_0, RelicId::Orrery);
    let deck = rm.run_state.master_deck.len();
    buy(&mut rm);
    for n in 0..5 {
        assert_eq!(rm.phase(), Phase::CardReward, "card reward {n}");
        assert_eq!(rm.card_reward_options().len(), 3, "card reward {n}");
        assert_eq!(
            rm.take_action(RunAction::PickCardReward { idx: Some(0) }),
            ActionResult::Advanced
        );
    }
    assert_eq!(rm.phase(), Phase::Shop);
    assert_eq!(rm.run_state.master_deck.len(), deck + 5);
    assert!(rm.run_state.pending_rewards.is_empty());
    assert!(rm.get_available_actions().contains(&RunAction::LeaveShop));
}

#[test]
fn orrery_card_rewards_can_be_skipped_back_to_the_shop() {
    let mut rm = shop_with(GameVersion::V0_111_0, RelicId::Orrery);
    let deck = rm.run_state.master_deck.len();
    buy(&mut rm);
    for _ in 0..5 {
        assert_eq!(
            rm.take_action(RunAction::PickCardReward { idx: None }),
            ActionResult::Advanced
        );
    }
    assert_eq!(rm.phase(), Phase::Shop);
    assert_eq!(rm.run_state.master_deck.len(), deck);
}

#[test]
fn cauldron_potions_are_collected_in_the_shop() {
    let mut rm = shop_with(GameVersion::V0_111_0, RelicId::Cauldron);
    assert!(rm.run_state.potions.is_empty());
    buy(&mut rm);
    assert_eq!(rm.phase(), Phase::Shop);
    assert!(rm.run_state.pending_rewards.is_empty());
    assert_eq!(
        rm.run_state.potions.len(),
        usize::try_from(rm.run_state.max_potion_slots).unwrap(),
        "the belt fills from the five potion rewards: {:?}",
        rm.run_state.potions
    );
}

#[test]
fn dollys_mirror_opens_a_deck_pick_and_duplicates_the_chosen_card() {
    let mut rm = shop_with(GameVersion::V0_111_0, RelicId::DollysMirror);
    let deck = rm.run_state.master_deck.clone();
    let rewards_before = rm.run_state.rng.rewards.counter();
    buy(&mut rm);
    let actions = rm.get_available_actions();
    assert_eq!(
        actions.len(),
        deck.len(),
        "one pick per non-Quest deck card, no skip"
    );
    assert!(actions
        .iter()
        .all(|a| matches!(a, RunAction::GrantDeckCard { .. })));
    // Bash sits at the end of the Ironclad starter deck.
    let pick = deck.len() - 1;
    assert_eq!(
        rm.take_action(RunAction::GrantDeckCard { idx: pick }),
        ActionResult::Advanced
    );
    assert_eq!(rm.run_state.master_deck.len(), deck.len() + 1);
    assert_eq!(
        rm.run_state.master_deck.last().map(|c| c.card_id),
        Some(deck[pick].card_id)
    );
    assert!(rm.run_state.pending_card_grants.is_empty());
    assert_eq!(rm.phase(), Phase::Shop);
    // C# picks the card; nothing is rolled.
    assert_eq!(rm.run_state.rng.rewards.counter(), rewards_before);
}

#[test]
fn kifuda_pick_can_stop_before_three() {
    let mut rm = shop_with(GameVersion::V0_111_0, RelicId::Kifuda);
    buy(&mut rm);
    let eligible = rm.enchant_eligible_deck_indices().len();
    assert!(eligible > 3);
    let actions = rm.get_available_actions();
    assert_eq!(
        actions.len(),
        eligible + 1,
        "every eligible card plus confirm"
    );
    assert_eq!(
        actions.last(),
        Some(&RunAction::EnchantDeckCard { idx: eligible })
    );
    assert_eq!(
        rm.take_action(RunAction::EnchantDeckCard { idx: 0 }),
        ActionResult::Advanced
    );
    assert_eq!(rm.run_state.pending_deck_enchants.len(), 2);
    let eligible = rm.enchant_eligible_deck_indices().len();
    assert_eq!(
        rm.take_action(RunAction::EnchantDeckCard { idx: eligible }),
        ActionResult::Advanced
    );
    assert!(rm.run_state.pending_deck_enchants.is_empty());
    assert_eq!(enchanted(&rm, EnchantmentId::Adroit), 1);
    assert_eq!(rm.phase(), Phase::Shop);
    assert!(rm.get_available_actions().contains(&RunAction::LeaveShop));
}

#[test]
fn kifuda_takes_up_to_three_distinct_cards() {
    let mut rm = shop_with(GameVersion::V0_111_0, RelicId::Kifuda);
    buy(&mut rm);
    for _ in 0..3 {
        assert_eq!(
            rm.take_action(RunAction::EnchantDeckCard { idx: 0 }),
            ActionResult::Advanced
        );
    }
    assert!(rm.run_state.pending_deck_enchants.is_empty());
    assert_eq!(enchanted(&rm, EnchantmentId::Adroit), 3);
    assert_eq!(rm.phase(), Phase::Shop);
}

#[test]
fn gnarled_hammer_pick_can_take_none() {
    let mut rm = shop_with(GameVersion::V0_111_0, RelicId::GnarledHammer);
    buy(&mut rm);
    let eligible = rm.enchant_eligible_deck_indices().len();
    assert!(eligible > 0);
    assert_eq!(
        rm.get_available_actions().last(),
        Some(&RunAction::EnchantDeckCard { idx: eligible })
    );
    assert_eq!(
        rm.take_action(RunAction::EnchantDeckCard { idx: eligible }),
        ActionResult::Advanced
    );
    assert!(rm.run_state.pending_deck_enchants.is_empty());
    assert_eq!(enchanted(&rm, EnchantmentId::Sharp), 0);
    assert_eq!(rm.phase(), Phase::Shop);
}

#[test]
fn punch_dagger_pick_is_mandatory() {
    let mut rm = shop_with(GameVersion::V0_111_0, RelicId::PunchDagger);
    buy(&mut rm);
    let eligible = rm.enchant_eligible_deck_indices().len();
    let actions = rm.get_available_actions();
    assert_eq!(actions.len(), eligible, "no confirm-early option");
    assert_eq!(
        rm.take_action(RunAction::EnchantDeckCard { idx: eligible }),
        ActionResult::InvalidForPhase
    );
    assert_eq!(
        rm.take_action(RunAction::EnchantDeckCard { idx: 0 }),
        ActionResult::Advanced
    );
    assert_eq!(enchanted(&rm, EnchantmentId::Momentum), 1);
    assert_eq!(rm.phase(), Phase::Shop);
}

#[test]
fn royal_stamp_shuffles_its_pool_on_niche_then_opens_a_mandatory_pick() {
    let mut rm = shop_with(GameVersion::V0_111_0, RelicId::RoyalStamp);
    let niche_before = rm.run_state.rng.niche.counter();
    buy(&mut rm);
    let eligible = rm.enchant_eligible_deck_indices().len();
    assert!(eligible > 1);
    // `UnstableShuffle` over the enchantable deck cards: one read per card but the first.
    assert_eq!(
        rm.run_state.rng.niche.counter(),
        niche_before + u32::try_from(eligible).unwrap() - 1
    );
    assert_eq!(rm.get_available_actions().len(), eligible);
    assert_eq!(
        rm.take_action(RunAction::EnchantDeckCard { idx: 0 }),
        ActionResult::Advanced
    );
    assert_eq!(enchanted(&rm, EnchantmentId::RoyallyApproved), 1);
    assert_eq!(rm.phase(), Phase::Shop);
}

/// v0.107.1 keeps the old shop path: the queued rewards stay on `pending_rewards`.
#[test]
fn v0107_shop_purchase_is_unchanged() {
    let mut rm = shop_with(GameVersion::V0_107_1, RelicId::Orrery);
    buy(&mut rm);
    assert_eq!(rm.phase(), Phase::Shop);
    assert_eq!(rm.run_state.pending_rewards.len(), 5);
}

/// TheCourier restocks the bought slot only after all five Orrery card rewards
/// have rolled (C# `RewardsSet.Offer` runs inside `OnTryPurchase`, then
/// `RestockAfterPurchase` reads `Rewards` in `RelicFactory.RollRarity`). The same
/// purchase without the Courier reads the identical card rolls; only the last
/// pick, which returns to the shop, adds the restock reads.
#[test]
fn courier_restock_follows_the_orrery_card_rewards() {
    let mut plain = shop_with(GameVersion::V0_111_0, RelicId::Orrery);
    let mut courier = shop_with(GameVersion::V0_111_0, RelicId::Orrery);
    courier.run_state.relics.push(RelicId::TheCourier);
    let sold_out = |rm: &RunManager| {
        !sts2_core::shop::is_shop_entry_available(rm.shop_inventory().unwrap().relics[0].price)
    };
    buy(&mut plain);
    buy(&mut courier);
    for n in 0..5 {
        assert_eq!(courier.phase(), Phase::CardReward, "card reward {n}");
        assert!(
            sold_out(&courier),
            "no restock while rewards are open ({n})"
        );
        assert_eq!(courier.card_reward_options(), plain.card_reward_options());
        assert_eq!(
            courier.run_state.rng.rewards.counter(),
            plain.run_state.rng.rewards.counter(),
            "card reward {n} rolled before any restock read"
        );
        for rm in [&mut plain, &mut courier] {
            assert_eq!(
                rm.take_action(RunAction::PickCardReward { idx: None }),
                ActionResult::Advanced
            );
        }
    }
    assert_eq!(courier.phase(), Phase::Shop);
    assert!(sold_out(&plain));
    assert!(
        !sold_out(&courier),
        "the Courier restocked the slot on return"
    );
    assert!(courier.run_state.rng.rewards.counter() > plain.run_state.rng.rewards.counter());
}

/// The shop drain and RoyalStamp's Niche shuffle are gated on the game version
/// only, so a non-interactive v0.111.0 host (runai / MCTS / parity) changes too:
/// Orrery's card rewards open in the shop and RoyalStamp reads Niche. The deck
/// selects stay on their non-interactive arms (Kifuda auto-enchants 3 cards,
/// DollysMirror duplicates a random card).
#[test]
fn non_interactive_v0111_host_also_gets_the_shop_rewards() {
    let mut rm = non_interactive(shop_with(GameVersion::V0_111_0, RelicId::Orrery));
    buy(&mut rm);
    assert_eq!(rm.phase(), Phase::CardReward);
    for _ in 0..5 {
        assert_eq!(
            rm.take_action(RunAction::PickCardReward { idx: None }),
            ActionResult::Advanced
        );
    }
    assert_eq!(rm.phase(), Phase::Shop);

    let mut rm = non_interactive(shop_with(GameVersion::V0_111_0, RelicId::RoyalStamp));
    let niche_before = rm.run_state.rng.niche.counter();
    buy(&mut rm);
    assert!(rm.run_state.rng.niche.counter() > niche_before);
    assert_eq!(enchanted(&rm, EnchantmentId::RoyallyApproved), 1);
    assert_eq!(rm.phase(), Phase::Shop);

    let mut rm = non_interactive(shop_with(GameVersion::V0_111_0, RelicId::Kifuda));
    buy(&mut rm);
    assert!(rm.run_state.pending_deck_enchants.is_empty());
    assert_eq!(enchanted(&rm, EnchantmentId::Adroit), 3);

    let mut rm = non_interactive(shop_with(GameVersion::V0_111_0, RelicId::DollysMirror));
    let deck = rm.run_state.master_deck.len();
    buy(&mut rm);
    assert!(rm.run_state.pending_card_grants.is_empty());
    assert_eq!(rm.run_state.master_deck.len(), deck + 1);
}

/// Lost Coffer and Massive Scroll also queue `AfterObtained` reward sentinels,
/// but both are `RelicRarity.Ancient` (`LostCoffer.cs:12`, `MassiveScroll.cs:17`):
/// the merchant stocks from the Common / Uncommon / Rare / Shop grab bags, so
/// they never reach a shop.
#[test]
fn lost_coffer_and_massive_scroll_are_never_shop_stock() {
    let ancient = [RelicId::LostCoffer, RelicId::MassiveScroll];
    for character in [
        CharacterId::Ironclad,
        CharacterId::Silent,
        CharacterId::Defect,
        CharacterId::Regent,
        CharacterId::Necrobinder,
    ] {
        let buckets = sts2_content::relics::grab_bag::grab_bag_buckets(character);
        for relics in buckets.values() {
            assert!(
                !relics.iter().any(|r| ancient.contains(r)),
                "{character:?} grab bag stocks an Ancient reward relic"
            );
        }
    }
    for relics in sts2_content::relics::grab_bag::shared_grab_bag_buckets().values() {
        assert!(!relics.iter().any(|r| ancient.contains(r)));
    }
}

/// Were one stocked anyway (a hand-built inventory), the generic shop drain would
/// surface Lost Coffer's card reward and potion and return to the shop.
#[test]
fn a_stocked_lost_coffer_would_open_its_rewards_in_the_shop() {
    let mut rm = shop_with(GameVersion::V0_111_0, RelicId::LostCoffer);
    buy(&mut rm);
    assert_eq!(rm.phase(), Phase::CardReward);
    assert_eq!(
        rm.run_state.potions.len(),
        1,
        "the potion reward is claimed"
    );
    assert_eq!(
        rm.take_action(RunAction::PickCardReward { idx: None }),
        ActionResult::Advanced
    );
    assert_eq!(rm.phase(), Phase::Shop);
    assert!(rm.run_state.pending_rewards.is_empty());
}
