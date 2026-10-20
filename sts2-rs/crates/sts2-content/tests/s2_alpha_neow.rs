//! Stage 2 bundle alpha (Neow) regression tests, pinned to the v0.111.0
//! decompile (`../sts2-wiki/decompiled/v0.111.0/`):
//!
//! - A3-a/b: the 10 baseline Neow-mismatch seeds (P0A1,3,6,11,20,23,24,25,26,28)
//!   reproduce the C# offers recorded in `logs/engine/p0/s2/baseline.md` 2.1
//!   (`Neow.cs:96-107` curse pool of 10, `:244-248` NeowsSacrifice removals).
//! - A3-c: `NeowsBones.GetValidRelics` is `Neow.AllPossibleOptions` minus
//!   `NeowsBones` and `MassiveScroll` (30 -> 28; `NeowsBones.cs:31-36`).
//! - Acquisition record for the two new relics: ID/name/offer plus the
//!   metadata-only `unported_effects` marker (A3-d effects are stage 5).
//! - #15: `NeowsBones` -> [`NewLeaf`, `ScrollBoxes`] opens the Scroll Boxes
//!   bundle pick once NewLeaf's transform resolves (C1Y118,
//!   `ScrollBoxes.cs:32-40`).

use sts2_content::relics::neow_relics::neows_bones_valid_pool;
use sts2_content::run_callbacks::{
    new_ironclad_run_manager, new_run_manager_for_character_from_string_seed,
};
use sts2_core::cards::CardId;
use sts2_core::potions::CharacterId;
use sts2_core::{
    neow_all_possible_options, ActionResult, DotNetRng, NeowOffers, NeowRelic, Phase, RelicId,
    RunAction,
};

/// `baseline.md` 2.1: C# (oracle, v0.111.0) `neow_offers` for the 10 seeds the
/// 8-curse Rust pool got wrong, as `[positive_1, positive_2, cursed]`.
const BASELINE_OFFERS: &[(&str, [NeowRelic; 3])] = &[
    (
        "P0A1",
        [
            NeowRelic::NeowsTorment,
            NeowRelic::PhialHolster,
            NeowRelic::SilkenTress,
        ],
    ),
    (
        "P0A3",
        [
            NeowRelic::NutritiousOyster,
            NeowRelic::PhialHolster,
            NeowRelic::DowsingRod,
        ],
    ),
    (
        "P0A6",
        [
            NeowRelic::ScrollBoxes,
            NeowRelic::NeowsTorment,
            NeowRelic::NeowsSacrifice,
        ],
    ),
    (
        "P0A11",
        [
            NeowRelic::LostCoffer,
            NeowRelic::PhialHolster,
            NeowRelic::HeftyTablet,
        ],
    ),
    (
        "P0A20",
        [
            NeowRelic::FishingRod,
            NeowRelic::PhialHolster,
            NeowRelic::PrecariousShears,
        ],
    ),
    (
        "P0A23",
        [
            NeowRelic::LavaRock,
            NeowRelic::GoldenPearl,
            NeowRelic::HeftyTablet,
        ],
    ),
    (
        "P0A24",
        [
            NeowRelic::NutritiousOyster,
            NeowRelic::ArcaneScroll,
            NeowRelic::NeowsSacrifice,
        ],
    ),
    (
        "P0A25",
        [
            NeowRelic::BoomingConch,
            NeowRelic::GoldenPearl,
            NeowRelic::DowsingRod,
        ],
    ),
    (
        "P0A26",
        [
            NeowRelic::NeowsTalisman,
            NeowRelic::PhialHolster,
            NeowRelic::DowsingRod,
        ],
    ),
    (
        "P0A28",
        [
            NeowRelic::GoldenPearl,
            NeowRelic::FishingRod,
            NeowRelic::NeowsSacrifice,
        ],
    ),
];

#[test]
fn baseline_neow_mismatch_seeds_reproduce_csharp_offers() {
    for (seed, expected) in BASELINE_OFFERS {
        let rm = new_run_manager_for_character_from_string_seed(seed, CharacterId::Ironclad)
            .with_neow_start();
        let got = rm.run_state.neow_offers.expect("neow offers rolled");
        assert_eq!(
            [got.positive_1, got.positive_2, got.cursed],
            *expected,
            "seed {seed}: Rust Neow offers must equal the C# oracle offers"
        );
    }
}

#[test]
fn neows_sacrifice_seeds_never_offer_phial_holster_or_lost_coffer() {
    for (seed, expected) in BASELINE_OFFERS {
        if expected[2] != NeowRelic::NeowsSacrifice {
            continue;
        }
        let rm = new_run_manager_for_character_from_string_seed(seed, CharacterId::Ironclad)
            .with_neow_start();
        let o = rm.run_state.neow_offers.unwrap();
        for p in [o.positive_1, o.positive_2] {
            assert_ne!(p, NeowRelic::PhialHolster, "seed {seed}");
            assert_ne!(p, NeowRelic::LostCoffer, "seed {seed}");
        }
    }
}

/// A3-c: 10 curses + 14 positives + 6 singles = 30 options; the single-player
/// valid pool drops `NeowsBones` (self) and `MassiveScroll` -> 28, derived from
/// `Neow.AllPossibleOptions`, in C# order.
#[test]
fn neows_bones_valid_pool_is_all_possible_options_minus_self_and_massive_scroll() {
    let all = neow_all_possible_options();
    assert_eq!(all.len(), 30);
    let pool = neows_bones_valid_pool();
    assert_eq!(pool.len(), 28);
    let expected: Vec<NeowRelic> = all
        .iter()
        .copied()
        .filter(|r| *r != NeowRelic::NeowsBones && *r != NeowRelic::MassiveScroll)
        .collect();
    assert_eq!(pool, expected);
    // The v0.111.0 additions are in the pool; the excluded two are not.
    assert!(pool.contains(&NeowRelic::DowsingRod));
    assert!(pool.contains(&NeowRelic::NeowsSacrifice));
    assert!(!pool.contains(&NeowRelic::NeowsBones));
    assert!(!pool.contains(&NeowRelic::MassiveScroll));
    // C# order: DowsingRod is the 2nd curse, NeowsSacrifice follows the
    // (excluded) NeowsBones.
    assert_eq!(pool[0], NeowRelic::CursedPearl);
    assert_eq!(pool[1], NeowRelic::DowsingRod);
    assert_eq!(pool[5], NeowRelic::NeowsSacrifice);
}

/// Acquisition of the two new relics leaves a metadata-only record and changes
/// nothing else (no deck, potion, gold or HP effect — those are stage 5).
#[test]
fn new_curse_relics_leave_unported_marker_and_no_state_change() {
    sts2_content::relics::dispatch::ensure_installed();
    for (relic, needle) in [
        (RelicId::DowsingRod, "Dowsing"),
        (RelicId::NeowsSacrifice, "Ambergris"),
    ] {
        let mut rm = new_ironclad_run_manager(0);
        let rs = &mut rm.run_state;
        let before_deck = rs.master_deck.clone();
        let (gold, hp, max_hp, potions) = (rs.gold, rs.current_hp, rs.max_hp, rs.potions.clone());
        assert!(rs.unported_effects.is_empty());
        rs.obtain_relic(relic);
        assert!(rs.relics.contains(&relic));
        assert_eq!(rs.unported_effects.len(), 1);
        assert_eq!(rs.unported_effects[0].relic, relic);
        assert!(rs.unported_effects[0].csharp_effect.contains(needle));
        assert_eq!(rs.master_deck, before_deck, "{relic:?}: no deck change");
        assert_eq!((rs.gold, rs.current_hp, rs.max_hp), (gold, hp, max_hp));
        assert_eq!(rs.potions, potions, "{relic:?}: no potion change");
    }
    // The marker is excluded from equality: clearing only it keeps the state equal.
    let mut rm = new_ironclad_run_manager(0);
    rm.run_state.obtain_relic(RelicId::DowsingRod);
    let mut cleared = rm.run_state.clone();
    cleared.unported_effects = sts2_core::runstate::UnportedEffectLog::default();
    assert!(cleared.unported_effects.is_empty());
    assert_eq!(
        rm.run_state, cleared,
        "unported_effects must not affect RunState equality"
    );
    // A ported relic leaves no marker.
    let mut rm = new_ironclad_run_manager(0);
    rm.run_state.obtain_relic(RelicId::NeowsTalisman);
    assert!(rm.run_state.unported_effects.is_empty());
}

#[test]
fn new_curse_relic_ids_and_names() {
    assert_eq!(RelicId::DowsingRod.to_python_name(), "DOWSING_ROD");
    assert_eq!(RelicId::NeowsSacrifice.to_python_name(), "NEOWS_SACRIFICE");
    assert_eq!(
        sts2_content::names::relic_id_from_str("DOWSING_ROD"),
        Some(RelicId::DowsingRod)
    );
    assert_eq!(
        sts2_content::names::relic_id_from_str("NEOWS_SACRIFICE"),
        Some(RelicId::NeowsSacrifice)
    );
}

// --- #15: NeowsBones -> [NewLeaf, ScrollBoxes] (C1Y118) -----------------

/// Find a `Rewards` stream seed whose `NeowsBones` shuffle puts `NewLeaf`
/// first and `ScrollBoxes` second (the C1Y118 chain). Replays exactly what
/// `listener_neows_bones` does: `Rewards.Shuffle(valid_pool)` (`NeowsBones.cs:41`).
fn rewards_seed_for_first_then_scroll_boxes(first: NeowRelic) -> u64 {
    (0..2_000_000_u64)
        .find(|&s| {
            let mut pool = neows_bones_valid_pool();
            DotNetRng::new(s).shuffle(&mut pool);
            pool[0] == first && pool[1] == NeowRelic::ScrollBoxes
        })
        .expect("a seed with the requested shuffle exists")
}

fn neows_bones_new_leaf_scroll_boxes_run() -> sts2_core::RunManager {
    neows_bones_first_then_scroll_boxes_run(NeowRelic::NewLeaf)
}

fn neows_bones_first_then_scroll_boxes_run(first: NeowRelic) -> sts2_core::RunManager {
    sts2_content::relics::dispatch::ensure_installed();
    let seed = rewards_seed_for_first_then_scroll_boxes(first);
    let mut rm = new_ironclad_run_manager(7).with_neow_start();
    rm.run_state.interactive_card_transform = true;
    rm.run_state.direct_event_relic_grant = true;
    rm.run_state.neow_offers = Some(NeowOffers {
        positive_1: NeowRelic::ArcaneScroll,
        positive_2: NeowRelic::BoomingConch,
        cursed: NeowRelic::NeowsBones,
    });
    rm.run_state.rng.rewards = DotNetRng::new(seed);
    rm
}

#[test]
fn neows_bones_new_leaf_then_scroll_boxes_opens_the_bundle_pick() {
    let mut rm = neows_bones_new_leaf_scroll_boxes_run();
    let deck_before = rm.run_state.master_deck.len();

    // Step 1 (C1Y118 step 1): choose_option idx2 = NEOWS_BONES.
    assert_eq!(
        rm.take_action(RunAction::EventChoice { idx: 2 }),
        ActionResult::Advanced
    );
    // NewLeaf (first shuffled relic) is in; its transform pick is pending;
    // ScrollBoxes (second) is owed but not yet granted.
    assert!(rm.run_state.relics.contains(&RelicId::NeowsBones));
    assert!(
        rm.run_state.relics.contains(&RelicId::NewLeaf),
        "relics {:?} deferred {:?}",
        rm.run_state.relics,
        rm.run_state.pending_post_pick_relic_grants
    );
    assert!(!rm.run_state.relics.contains(&RelicId::ScrollBoxes));
    assert_eq!(
        rm.run_state.pending_post_pick_relic_grants,
        vec![RelicId::ScrollBoxes]
    );
    assert!(!rm.run_state.pending_deck_transforms.is_empty());
    assert!(rm
        .get_available_actions()
        .iter()
        .all(|a| matches!(a, RunAction::TransformDeckCard { .. })));

    // Step 2 (C1Y118 step 2): select_cards idx3 on the transform. C# opens
    // `bundle_select` here (ScrollBoxes.AfterObtained -> FromChooseABundleScreen).
    assert_eq!(
        rm.take_action(RunAction::TransformDeckCard { idx: 3 }),
        ActionResult::Advanced
    );
    assert!(rm.run_state.relics.contains(&RelicId::ScrollBoxes));
    assert_eq!(rm.phase(), Phase::CardReward, "bundle screen must be open");
    let bundles = rm.card_reward_bundles().expect("bundle_select is surfaced");
    assert_eq!(bundles.len(), 2, "ScrollBoxes offers 2 bundles");
    assert!(bundles.iter().all(|b| b.len() == 3), "3 cards per bundle");
    assert!(rm.run_state.pending_bundles.is_empty());
    // The NeowsBones curse waits for the bundle pick (C# adds it after
    // `RewardsSet.Offer()` returns).
    assert_eq!(rm.run_state.master_deck.len(), deck_before);

    // Pick bundle 0: its 3 cards join the deck, then the deferred curse lands.
    let chosen: Vec<CardId> = bundles[0].clone();
    assert_eq!(
        rm.take_action(RunAction::PickCardReward { idx: Some(0) }),
        ActionResult::Advanced
    );
    assert_eq!(rm.phase(), Phase::MapChoice);
    assert!(rm.card_reward_bundles().is_none());
    let deck: Vec<CardId> = rm.run_state.master_deck.iter().map(|c| c.card_id).collect();
    assert_eq!(deck.len(), deck_before + 3 + 1, "3 bundle cards + 1 curse");
    for c in chosen {
        assert!(deck.contains(&c), "bundle card {c:?} added to the deck");
    }
}

/// Reward-first variant: NeowsBones -> [LostCoffer, ScrollBoxes]. LostCoffer
/// opens a card reward; ScrollBoxes is owed until that pick resolves, then its
/// bundle screen must open (the `advance_after_card_reward` surfacing site).
#[test]
fn neows_bones_reward_first_then_scroll_boxes_opens_the_bundle_pick() {
    let mut rm = neows_bones_first_then_scroll_boxes_run(NeowRelic::LostCoffer);
    assert_eq!(
        rm.take_action(RunAction::EventChoice { idx: 2 }),
        ActionResult::Advanced
    );
    assert!(rm.run_state.relics.contains(&RelicId::LostCoffer));
    assert!(!rm.run_state.relics.contains(&RelicId::ScrollBoxes));
    assert_eq!(
        rm.phase(),
        Phase::CardReward,
        "LostCoffer card reward is open"
    );
    assert!(rm.card_reward_bundles().is_none());

    assert_eq!(
        rm.take_action(RunAction::PickCardReward { idx: Some(0) }),
        ActionResult::Advanced
    );
    assert!(rm.run_state.relics.contains(&RelicId::ScrollBoxes));
    assert_eq!(rm.phase(), Phase::CardReward, "bundle screen must be open");
    let bundles = rm.card_reward_bundles().expect("bundle_select is surfaced");
    assert_eq!(bundles.len(), 2);
    assert!(bundles.iter().all(|b| b.len() == 3));
}
