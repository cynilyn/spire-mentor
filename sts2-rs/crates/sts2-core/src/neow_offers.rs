//! Neow event option-roll RNG algorithm. Sub-bead `sts2-rs-qq9og`
//! (R3.1.1 under umbrella `sts2-rs-nr8l4`).
//!
//! Port pin:
//! `../sts2-wiki/decompiled/v0.111.0/MegaCrit.Sts2.Core.Models.Events/Neow.cs:216-286`
//! (`Neow.GenerateInitialOptions`, no-modifiers branch; stage 2 bundle alpha
//! re-pinned the pools to v0.111.0: `CurseOptions[10]` `:96-107`).
//!
//! Pure function: [`roll_neow_offers`] takes a per-event RNG (seeded
//! per v0.111.0 `EventModel.cs:197` — `RunState.Rng.Seed + slot 0 +
//! XxHash64("NEOW")`; wiring lives in sub-bead
//! `sts2-rs-14cid`) and returns the 3 relic offers, RNG-faithful to
//! the C# algorithm (same `next_int` / `next_bool` / `shuffle` call
//! sequence).
//!
//! The relic IDs are returned as [`NeowRelic`], a closed enum mirroring
//! the C# pool. Every variant now maps to a first-class [`RelicId`]:
//! `PhialHolster` / `WingedBoots` / `NeowsTalisman` (bead `sts2-rs-4rwv4`,
//! W9a-c), `NeowsBones` (bead `sts2-rs-wjuoi`, W9e), and `HeftyTablet`
//! (bead `sts2-rs-w6hdj`, W9d — 3-Rare card-select + always-add Injury
//! via the run-side pending-card-select surface).

use crate::DotNetRng as Rng;
use crate::RelicId;

/// Closed enum of every relic that can appear in a Neow offer.
/// Mirrors the C# pool at `Neow.cs:67-103`. Every variant resolves to a
/// first-class [`RelicId`] via [`Self::to_relic_id`] (W9a-e).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NeowRelic {
    // ── Positive base pool (v0.107.1 Neow.cs:66-82) ───────────────────
    ArcaneScroll,
    BoomingConch,
    GoldenPearl,
    LeadPaperweight,
    LostCoffer,
    NeowsTorment,
    NewLeaf,
    PreciseScissors,
    PhialHolster,
    WingedBoots,
    MassiveScroll,
    /// v0.107.1 (bead `sts2-rs-v0107-content-port-wb8lk.6`): FishingRod
    /// and Kaleidoscope joined the positive pool, and `ScrollBoxes`
    /// moved here from the v0.103.2 curse-pool add-in slot.
    FishingRod,
    Kaleidoscope,
    // ── Positive 50/50 add-ins (Neow.cs:245-271) ──────────────────────
    LavaRock,
    SmallCapsule,
    NutritiousOyster,
    StoneHumidifier,
    NeowsTalisman,
    Pomander,
    // ── Curse pool (v0.107.1 Neow.cs:96-106) ──────────────────────────
    CursedPearl,
    HeftyTablet,
    LargeCapsule,
    LeafyPoultice,
    PrecariousShears,
    SilverCrucible,
    NeowsBones,
    /// v0.107.1 (bead `sts2-rs-v0107-content-port-wb8lk.6`): new curse
    /// option. On pickup loses all gold; the first card reward after is
    /// Glam-enchanted (one-time).
    SilkenTress,
    /// v0.111.0 (stage 2 bundle alpha, ledger A3-a): new curse option
    /// (`Neow.cs:96-107`). Offer + acquisition record only; the pickup effect
    /// (quest card `Dowsing`, `DowsingRod.cs:21-22`) is unported (stage 5).
    DowsingRod,
    /// v0.111.0 (stage 2 bundle alpha, ledger A3-a/b): new curse option
    /// (`Neow.cs:96-107`) that removes `PhialHolster` and `LostCoffer` from the
    /// positive pool (`Neow.cs:244-248`). Offer + acquisition record only; the
    /// pickup effect (potion `Ambergris` + curse `Guilty`,
    /// `NeowsSacrifice.cs:31-33`) is unported (stage 5).
    NeowsSacrifice,
    /// v0.107.1: `ScrollBoxes` moved out of the curse pool into the
    /// positive pool (it is a static `PositiveOptions` member now). The
    /// variant stays for exhaustive pattern-matching.
    ScrollBoxes,
}

impl NeowRelic {
    /// Convert to the matching [`RelicId`]. Every Neow-offer variant now
    /// has a first-class `RelicId` counterpart (W9a-e); the return type
    /// stays `Option` for call-site stability and forward-compat with any
    /// future Neow option that lands ahead of its `RelicId`.
    #[must_use]
    pub fn to_relic_id(self) -> Option<RelicId> {
        Some(match self {
            Self::ArcaneScroll => RelicId::ArcaneScroll,
            Self::BoomingConch => RelicId::BoomingConch,
            Self::GoldenPearl => RelicId::GoldenPearl,
            Self::LeadPaperweight => RelicId::LeadPaperweight,
            Self::LostCoffer => RelicId::LostCoffer,
            Self::NeowsTorment => RelicId::NeowsTorment,
            Self::NewLeaf => RelicId::NewLeaf,
            Self::PreciseScissors => RelicId::PreciseScissors,
            Self::MassiveScroll => RelicId::MassiveScroll,
            Self::LavaRock => RelicId::LavaRock,
            Self::SmallCapsule => RelicId::SmallCapsule,
            Self::NutritiousOyster => RelicId::NutritiousOyster,
            Self::StoneHumidifier => RelicId::StoneHumidifier,
            Self::Pomander => RelicId::Pomander,
            Self::CursedPearl => RelicId::CursedPearl,
            Self::LargeCapsule => RelicId::LargeCapsule,
            Self::LeafyPoultice => RelicId::LeafyPoultice,
            Self::PrecariousShears => RelicId::PrecariousShears,
            Self::SilverCrucible => RelicId::SilverCrucible,
            Self::ScrollBoxes => RelicId::ScrollBoxes,
            Self::PhialHolster => RelicId::PhialHolster,
            Self::WingedBoots => RelicId::WingedBoots,
            Self::NeowsTalisman => RelicId::NeowsTalisman,
            Self::NeowsBones => RelicId::NeowsBones,
            Self::HeftyTablet => RelicId::HeftyTablet,
            Self::FishingRod => RelicId::FishingRod,
            Self::Kaleidoscope => RelicId::Kaleidoscope,
            Self::SilkenTress => RelicId::SilkenTress,
            Self::DowsingRod => RelicId::DowsingRod,
            Self::NeowsSacrifice => RelicId::NeowsSacrifice,
        })
    }

    /// SCREAMING_SNAKE_CASE label matching the scraped-corpus
    /// `relic_id` field. Stable across versions; the 5 missing relics
    /// emit their canonical SCREAMING_SNAKE_CASE name so parity dumps
    /// can compare against the scraped JSON before `RelicId` catches up.
    #[must_use]
    pub fn snake_case_id(self) -> &'static str {
        match self {
            Self::ArcaneScroll => "ARCANE_SCROLL",
            Self::BoomingConch => "BOOMING_CONCH",
            Self::GoldenPearl => "GOLDEN_PEARL",
            Self::LeadPaperweight => "LEAD_PAPERWEIGHT",
            Self::LostCoffer => "LOST_COFFER",
            Self::NeowsTorment => "NEOWS_TORMENT",
            Self::NewLeaf => "NEW_LEAF",
            Self::PreciseScissors => "PRECISE_SCISSORS",
            Self::PhialHolster => "PHIAL_HOLSTER",
            Self::WingedBoots => "WINGED_BOOTS",
            Self::MassiveScroll => "MASSIVE_SCROLL",
            Self::LavaRock => "LAVA_ROCK",
            Self::SmallCapsule => "SMALL_CAPSULE",
            Self::NutritiousOyster => "NUTRITIOUS_OYSTER",
            Self::StoneHumidifier => "STONE_HUMIDIFIER",
            Self::NeowsTalisman => "NEOWS_TALISMAN",
            Self::Pomander => "POMANDER",
            Self::CursedPearl => "CURSED_PEARL",
            Self::HeftyTablet => "HEFTY_TABLET",
            Self::LargeCapsule => "LARGE_CAPSULE",
            Self::LeafyPoultice => "LEAFY_POULTICE",
            Self::PrecariousShears => "PRECARIOUS_SHEARS",
            Self::SilverCrucible => "SILVER_CRUCIBLE",
            Self::NeowsBones => "NEOWS_BONES",
            Self::ScrollBoxes => "SCROLL_BOXES",
            Self::FishingRod => "FISHING_ROD",
            Self::Kaleidoscope => "KALEIDOSCOPE",
            Self::SilkenTress => "SILKEN_TRESS",
            Self::DowsingRod => "DOWSING_ROD",
            Self::NeowsSacrifice => "NEOWS_SACRIFICE",
        }
    }
}

/// The 3 offers Neow presents at run-start: 2 positive relics + 1
/// curse. Mirrors C# `Neow.GenerateInitialOptions` return shape
/// (`[positive_1, positive_2, cursed]` — the curse is always the third
/// option in the UI).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NeowOffers {
    pub positive_1: NeowRelic,
    pub positive_2: NeowRelic,
    pub cursed: NeowRelic,
}

/// Base positive pool — Neow.cs:67-80. Listed in the C# array as 11
/// entries including `MassiveScroll`, but `MassiveScroll.IsAllowed`
/// (`MassiveScroll.cs:19`) gates on `runState.Players.Count > 1` so
/// it is filtered out by the `RemoveAll(!IsAllowed)` pass at
/// Neow.cs:225-227 / :272-276 in any single-player run. Since
/// `sts2-rs` only models single-player at this pin (and the entire
/// run-loop assumes `players: [Ironclad]`), we drop `MassiveScroll`
/// from the static pool directly. (Should multi-player land later,
/// gate it on `state.players.len() > 1` here.)
///
/// **v0.107.1** (bead `sts2-rs-v0107-content-port-wb8lk.6`): the C#
/// `PositiveOptions` array grew 11→14 — `FishingRod`, `Kaleidoscope`,
/// and `ScrollBoxes` (moved in from the v0.103.2 curse add-in). Listed
/// here in the exact C# array order (`Neow.cs:66-82`) so the
/// `next_item` / `UnstableShuffle` RNG sequence stays oracle-faithful,
/// minus `MassiveScroll` (single-player `IsAllowedAtNeow == false`).
/// `Kaleidoscope.IsAllowedAtNeow` additionally requires every character
/// card pool unlocked; the run-start sim models a fully-unlocked roster,
/// so it is retained here (matching the v0.107.1 oracle).
const POSITIVE_POOL: &[NeowRelic] = &[
    NeowRelic::ArcaneScroll,
    NeowRelic::BoomingConch,
    NeowRelic::FishingRod,
    NeowRelic::GoldenPearl,
    NeowRelic::Kaleidoscope,
    NeowRelic::LeadPaperweight,
    NeowRelic::LostCoffer,
    NeowRelic::NeowsTorment,
    NeowRelic::NewLeaf,
    NeowRelic::PhialHolster,
    NeowRelic::PreciseScissors,
    NeowRelic::ScrollBoxes,
    NeowRelic::WingedBoots,
];

/// Curse pool — **v0.111.0** `Neow.cs:96-107` `CurseOptions[10]`, in the
/// exact C# array order so the `next_item` curse draw is oracle-faithful
/// (bead `sts2-rs-v0107-content-port-wb8lk.6`; stage 2 bundle alpha A3-a).
/// The array grew 7→8 with `SilkenTress` (v0.107.1) and 8→10 with
/// `DowsingRod` and `NeowsSacrifice` (v0.111.0); `ScrollBoxes` left the curse
/// pool (it is a static `PositiveOptions` member — see [`POSITIVE_POOL`]).
/// None of the ten is filtered by `IsAllowedAtNeow` in single player (only
/// `Kaleidoscope`/`ScrollBoxes` override it, and neither is a curse), so the
/// pre-`NextItem` `RemoveAll` (`Neow.cs:219-224`) is a no-op here.
const CURSE_POOL: &[NeowRelic] = &[
    NeowRelic::CursedPearl,
    NeowRelic::DowsingRod,
    NeowRelic::HeftyTablet,
    NeowRelic::LargeCapsule,
    NeowRelic::LeafyPoultice,
    NeowRelic::NeowsBones,
    NeowRelic::NeowsSacrifice,
    NeowRelic::PrecariousShears,
    NeowRelic::SilkenTress,
    NeowRelic::SilverCrucible,
];

/// `Neow.PositiveOptions` in the exact C# array order, **including**
/// `MassiveScroll` (`Neow.cs:66-81`, 14 entries). [`POSITIVE_POOL`] is this
/// list minus `MassiveScroll` (single-player `IsAllowed == false`); the full
/// list is kept so [`neow_all_possible_options`] can reproduce
/// `Neow.AllPossibleOptions` (`Neow.cs:49-64`) for `NeowsBones`.
const POSITIVE_OPTIONS: &[NeowRelic] = &[
    NeowRelic::ArcaneScroll,
    NeowRelic::BoomingConch,
    NeowRelic::FishingRod,
    NeowRelic::GoldenPearl,
    NeowRelic::Kaleidoscope,
    NeowRelic::LeadPaperweight,
    NeowRelic::LostCoffer,
    NeowRelic::MassiveScroll,
    NeowRelic::NeowsTorment,
    NeowRelic::NewLeaf,
    NeowRelic::PhialHolster,
    NeowRelic::PreciseScissors,
    NeowRelic::ScrollBoxes,
    NeowRelic::WingedBoots,
];

/// The six single add-in options appended to `AllPossibleOptions`
/// (`Neow.cs:55-61`, `AddRange` order: LavaRock, NeowsTalisman,
/// NutritiousOyster, Pomander, SmallCapsule, StoneHumidifier).
const SINGLE_OPTIONS: &[NeowRelic] = &[
    NeowRelic::LavaRock,
    NeowRelic::NeowsTalisman,
    NeowRelic::NutritiousOyster,
    NeowRelic::Pomander,
    NeowRelic::SmallCapsule,
    NeowRelic::StoneHumidifier,
];

/// `Neow.AllPossibleOptions` (`Neow.cs:49-64`): `CurseOptions` (10), then
/// `PositiveOptions` (14), then the six single add-in options — 30 relics in
/// C# order, before any `IsAllowedAtNeow` filtering. `NeowsBones.GetValidRelics`
/// (`NeowsBones.cs:31-36`) derives its shuffle pool from this list, so the pool
/// follows the Neow option lists instead of being hand-copied.
#[must_use]
pub fn neow_all_possible_options() -> Vec<NeowRelic> {
    let mut all =
        Vec::with_capacity(CURSE_POOL.len() + POSITIVE_OPTIONS.len() + SINGLE_OPTIONS.len());
    all.extend_from_slice(CURSE_POOL);
    all.extend_from_slice(POSITIVE_OPTIONS);
    all.extend_from_slice(SINGLE_OPTIONS);
    all
}

/// Inline `Rng.NextItem(IList<T>)` — uniform pick over a non-empty
/// slice. C# `Rng.NextItem` calls `NextInt(0, list.Count - 1)`
/// (inclusive both ends); our `next_int(low, high)` matches that
/// inclusive contract (see `dotnet_rng.rs::Rng::next_int`).
fn next_item<T: Copy>(rng: &mut Rng, lst: &[T]) -> T {
    assert!(!lst.is_empty(), "next_item on empty slice");
    let last = i32::try_from(lst.len() - 1).expect("pool size fits in i32");
    let idx = rng.next_int(0, last);
    lst[idx as usize]
}

/// Port of C# `Neow.GenerateInitialOptions` (no-modifiers branch).
///
/// Caller supplies a per-event RNG seeded per
/// `EventModel.cs:193`. The function performs exactly the same RNG
/// operations as the C# code:
///
/// 1. `next_int` to pick one curse from [`CURSE_POOL`].
/// 2. Up to 3 `next_bool` calls to choose between paired add-ins
///    (LavaRock/SmallCapsule — *suppressed* iff `cursed == LargeCapsule*,
///    NutritiousOyster/StoneHumidifier, NeowsTalisman/Pomander).
/// 3. `shuffle` (== C# `UnstableShuffle`) on the positive pool.
///
/// Returns 2 positives + the curse. The function does NOT apply a runtime
/// `IsAllowedAtNeow` filter pass: v0.107.1 renamed the Neow gate
/// `IsAllowed` → `IsAllowedAtNeow` (`Neow.cs:222,272`,
/// `RelicModel.cs:444`), but for the SINGLE-PLAYER, fully-unlocked sim the
/// gate is statically resolved into the pool composition above —
///   * `MassiveScroll.IsAllowed == (Players > 1)` ⇒ false ⇒ pre-dropped from
///     [`POSITIVE_POOL`] (it would be removed by the RemoveAll-before-shuffle
///     pass anyway, so the shuffle RNG sequence is unaffected);
///   * `WingedBoots` / `SilverCrucible` `IsAllowed == (Players == 1)` ⇒ true ⇒
///     retained;
///   * `Kaleidoscope.IsAllowedAtNeow` requires every character card pool
///     unlocked, and `ScrollBoxes.IsAllowedAtNeow` requires
///     `CanGenerateBundles` (≥4 commons + ≥2 uncommons) — both true for the
///     fully-unlocked roster + every shipped character pool, so retained.
/// No remaining pool relic gates. If a future addition introduces a Neow
/// relic that is gated in single-player, add a `.retain(...)` pass between
/// pool construction and `next_item`/`shuffle` (curse pool filtered BEFORE the
/// curse `next_item`, positives AFTER the add-ins and BEFORE the shuffle, per
/// `Neow.cs:218-223` / `:269-275`).
#[must_use]
pub fn roll_neow_offers(rng: &mut Rng) -> NeowOffers {
    // (1) curse pick — Neow.cs:217 + :227.
    let cursed = next_item(rng, CURSE_POOL);

    // (2) base positive pool, drop curse↔positive conflicts
    //     (Neow.cs:228-244).
    let mut positive_pool: Vec<NeowRelic> = POSITIVE_POOL.to_vec();
    match cursed {
        NeowRelic::CursedPearl => positive_pool.retain(|r| *r != NeowRelic::GoldenPearl),
        NeowRelic::HeftyTablet => positive_pool.retain(|r| *r != NeowRelic::ArcaneScroll),
        NeowRelic::LeafyPoultice => positive_pool.retain(|r| *r != NeowRelic::NewLeaf),
        NeowRelic::PrecariousShears => {
            positive_pool.retain(|r| *r != NeowRelic::PreciseScissors);
        }
        // Neow.cs:244-248: NeowsSacrifice removes PhialHolster, then LostCoffer.
        NeowRelic::NeowsSacrifice => {
            positive_pool.retain(|r| *r != NeowRelic::PhialHolster);
            positive_pool.retain(|r| *r != NeowRelic::LostCoffer);
        }
        _ => {}
    }

    // (3) LavaRock vs SmallCapsule — suppressed iff cursed is
    //     LargeCapsule (Neow.cs:245-255). NB: the C# branch consumes
    //     ONE `NextBool` only when the conditional fires. The Rust
    //     translation must do the same — DO NOT call `next_bool()`
    //     unconditionally.
    if cursed != NeowRelic::LargeCapsule {
        positive_pool.push(if rng.next_bool() {
            NeowRelic::LavaRock
        } else {
            NeowRelic::SmallCapsule
        });
    }

    // (4) NutritiousOyster vs StoneHumidifier — unconditional
    //     (Neow.cs:256-263).
    positive_pool.push(if rng.next_bool() {
        NeowRelic::NutritiousOyster
    } else {
        NeowRelic::StoneHumidifier
    });

    // (5) NeowsTalisman vs Pomander — unconditional (Neow.cs:264-271).
    positive_pool.push(if rng.next_bool() {
        NeowRelic::NeowsTalisman
    } else {
        NeowRelic::Pomander
    });

    // (6) UnstableShuffle (Neow.cs:278). `Rng::shuffle` is exactly
    //     UnstableShuffle (Fisher-Yates with `next_int(0, idx)`); see
    //     `dotnet_rng.rs::Rng::shuffle` doc-comment.
    rng.shuffle(&mut positive_pool);

    NeowOffers {
        positive_1: positive_pool[0],
        positive_2: positive_pool[1],
        cursed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_produces_same_offers() {
        let mut a = Rng::with_name(12345, "Neow");
        let mut b = Rng::with_name(12345, "Neow");
        assert_eq!(roll_neow_offers(&mut a), roll_neow_offers(&mut b));
    }

    #[test]
    fn cursed_always_drawn_from_curse_pool() {
        for seed in 0..200 {
            let mut rng = Rng::with_name(seed, "Neow");
            let o = roll_neow_offers(&mut rng);
            assert!(
                CURSE_POOL.contains(&o.cursed),
                "seed {seed}: cursed = {:?}",
                o.cursed
            );
        }
    }

    #[test]
    fn positives_disjoint_from_curse() {
        // Positive and curse pools share no variants; both positives
        // must be from the positive side (base pool or one of the 3
        // 50/50 add-ins).
        let positive_universe: &[NeowRelic] = &[
            NeowRelic::ArcaneScroll,
            NeowRelic::BoomingConch,
            NeowRelic::FishingRod,
            NeowRelic::GoldenPearl,
            NeowRelic::Kaleidoscope,
            NeowRelic::LeadPaperweight,
            NeowRelic::LostCoffer,
            NeowRelic::NeowsTorment,
            NeowRelic::NewLeaf,
            NeowRelic::PreciseScissors,
            NeowRelic::PhialHolster,
            NeowRelic::ScrollBoxes,
            NeowRelic::WingedBoots,
            NeowRelic::MassiveScroll,
            NeowRelic::LavaRock,
            NeowRelic::SmallCapsule,
            NeowRelic::NutritiousOyster,
            NeowRelic::StoneHumidifier,
            NeowRelic::NeowsTalisman,
            NeowRelic::Pomander,
        ];
        for seed in 0..200 {
            let mut rng = Rng::with_name(seed, "Neow");
            let o = roll_neow_offers(&mut rng);
            assert!(positive_universe.contains(&o.positive_1));
            assert!(positive_universe.contains(&o.positive_2));
        }
    }

    #[test]
    fn three_offers_all_distinct() {
        for seed in 0..1000 {
            let mut rng = Rng::with_name(seed, "Neow");
            let o = roll_neow_offers(&mut rng);
            assert_ne!(o.positive_1, o.positive_2, "seed {seed}");
            assert_ne!(o.positive_1, o.cursed, "seed {seed}");
            assert_ne!(o.positive_2, o.cursed, "seed {seed}");
        }
    }

    #[test]
    fn conflict_pairs_never_co_occur() {
        // Neow.cs:228-244 removes the conflicting positive when the
        // curse is rolled. After 5000 seeds we should see at least one
        // hit on each pair (pigeon-hole), and zero violations.
        let mut saw_cursed_pearl = false;
        let mut saw_hefty_tablet = false;
        let mut saw_leafy = false;
        let mut saw_shears = false;
        for seed in 0..5000 {
            let mut rng = Rng::with_name(seed, "Neow");
            let o = roll_neow_offers(&mut rng);
            match o.cursed {
                NeowRelic::CursedPearl => {
                    saw_cursed_pearl = true;
                    assert_ne!(o.positive_1, NeowRelic::GoldenPearl);
                    assert_ne!(o.positive_2, NeowRelic::GoldenPearl);
                }
                NeowRelic::HeftyTablet => {
                    saw_hefty_tablet = true;
                    assert_ne!(o.positive_1, NeowRelic::ArcaneScroll);
                    assert_ne!(o.positive_2, NeowRelic::ArcaneScroll);
                }
                NeowRelic::LeafyPoultice => {
                    saw_leafy = true;
                    assert_ne!(o.positive_1, NeowRelic::NewLeaf);
                    assert_ne!(o.positive_2, NeowRelic::NewLeaf);
                }
                NeowRelic::PrecariousShears => {
                    saw_shears = true;
                    assert_ne!(o.positive_1, NeowRelic::PreciseScissors);
                    assert_ne!(o.positive_2, NeowRelic::PreciseScissors);
                }
                _ => {}
            }
        }
        assert!(saw_cursed_pearl);
        assert!(saw_hefty_tablet);
        assert!(saw_leafy);
        assert!(saw_shears);
    }

    #[test]
    fn large_capsule_suppresses_lava_rock_and_small_capsule() {
        // Neow.cs:245-255: if cursed == LargeCapsule, the LavaRock/
        // SmallCapsule add-in is skipped. Neither should appear among
        // the positives in that case.
        let mut saw = false;
        for seed in 0..5000 {
            let mut rng = Rng::with_name(seed, "Neow");
            let o = roll_neow_offers(&mut rng);
            if o.cursed == NeowRelic::LargeCapsule {
                saw = true;
                assert_ne!(o.positive_1, NeowRelic::LavaRock);
                assert_ne!(o.positive_2, NeowRelic::LavaRock);
                assert_ne!(o.positive_1, NeowRelic::SmallCapsule);
                assert_ne!(o.positive_2, NeowRelic::SmallCapsule);
            }
        }
        assert!(saw, "no seed in 0..5000 rolled LargeCapsule curse");
    }

    #[test]
    fn to_relic_id_some_for_supported_variants() {
        assert_eq!(
            NeowRelic::ArcaneScroll.to_relic_id(),
            Some(RelicId::ArcaneScroll)
        );
        assert_eq!(
            NeowRelic::CursedPearl.to_relic_id(),
            Some(RelicId::CursedPearl)
        );
        assert_eq!(NeowRelic::LavaRock.to_relic_id(), Some(RelicId::LavaRock));
    }

    /// `Neow.AllPossibleOptions` is `CurseOptions` + `PositiveOptions` + 6
    /// singles (`Neow.cs:49-64`): 10 + 14 + 6 = 30, `POSITIVE_POOL` being the
    /// 14 minus the single-player-gated `MassiveScroll`.
    #[test]
    fn all_possible_options_is_curse_positive_singles_in_decomp_order() {
        let all = neow_all_possible_options();
        assert_eq!(all.len(), 30);
        assert_eq!(&all[..10], CURSE_POOL);
        assert_eq!(&all[10..24], POSITIVE_OPTIONS);
        assert_eq!(&all[24..], SINGLE_OPTIONS);
        let without_massive: Vec<NeowRelic> = POSITIVE_OPTIONS
            .iter()
            .copied()
            .filter(|r| *r != NeowRelic::MassiveScroll)
            .collect();
        assert_eq!(POSITIVE_POOL, without_massive.as_slice());
        let distinct: std::collections::HashSet<_> = all.iter().collect();
        assert_eq!(distinct.len(), 30, "no relic listed twice");
    }

    /// v0.107.1 `Neow.cs:66-82` `PositiveOptions[14]` in C# array order, minus
    /// `MassiveScroll` (single-player `IsAllowed`/`IsAllowedAtNeow == false` —
    /// removed by the RemoveAll pass before the shuffle, so dropping it from
    /// the static pool is RNG-faithful). The 3 v0.107.1 additions
    /// (FishingRod, Kaleidoscope, ScrollBoxes) must sit at their decomp indices.
    #[test]
    fn positive_pool_matches_v0107_decomp_order() {
        assert_eq!(
            POSITIVE_POOL,
            &[
                NeowRelic::ArcaneScroll,
                NeowRelic::BoomingConch,
                NeowRelic::FishingRod,
                NeowRelic::GoldenPearl,
                NeowRelic::Kaleidoscope,
                NeowRelic::LeadPaperweight,
                NeowRelic::LostCoffer,
                NeowRelic::NeowsTorment,
                NeowRelic::NewLeaf,
                NeowRelic::PhialHolster,
                NeowRelic::PreciseScissors,
                NeowRelic::ScrollBoxes,
                NeowRelic::WingedBoots,
            ],
        );
        // ScrollBoxes is a POSITIVE member in v0.107.1 (was a v0.103.2 curse
        // add-in); it must NOT appear in the curse pool.
        assert!(!CURSE_POOL.contains(&NeowRelic::ScrollBoxes));
        assert!(POSITIVE_POOL.contains(&NeowRelic::ScrollBoxes));
        // MassiveScroll is single-player-gated → never in the rolled pool.
        assert!(!POSITIVE_POOL.contains(&NeowRelic::MassiveScroll));
    }

    /// v0.111.0 `Neow.cs:96-107` `CurseOptions[10]` in C# array order. The
    /// 8→10 growth adds `DowsingRod` (slot 1) and `NeowsSacrifice` (slot 6).
    #[test]
    fn curse_pool_matches_v0111_decomp_order() {
        assert_eq!(
            CURSE_POOL,
            &[
                NeowRelic::CursedPearl,
                NeowRelic::DowsingRod,
                NeowRelic::HeftyTablet,
                NeowRelic::LargeCapsule,
                NeowRelic::LeafyPoultice,
                NeowRelic::NeowsBones,
                NeowRelic::NeowsSacrifice,
                NeowRelic::PrecariousShears,
                NeowRelic::SilkenTress,
                NeowRelic::SilverCrucible,
            ],
        );
    }

    /// `Neow.cs:244-248`: a `NeowsSacrifice` curse removes `PhialHolster` and
    /// `LostCoffer` from the positive pool before the add-ins and the shuffle.
    /// Checked over 20k seeds: neither relic ever co-occurs with the curse,
    /// while both still appear alongside other curses.
    #[test]
    fn neows_sacrifice_removes_phial_holster_and_lost_coffer() {
        let (mut saw_sacrifice, mut saw_phial_other, mut saw_coffer_other) = (0, false, false);
        for seed in 0..20_000 {
            let mut rng = Rng::with_name(seed, "Neow");
            let o = roll_neow_offers(&mut rng);
            if o.cursed == NeowRelic::NeowsSacrifice {
                saw_sacrifice += 1;
                for p in [o.positive_1, o.positive_2] {
                    assert_ne!(p, NeowRelic::PhialHolster, "seed {seed}");
                    assert_ne!(p, NeowRelic::LostCoffer, "seed {seed}");
                }
            } else {
                for p in [o.positive_1, o.positive_2] {
                    saw_phial_other |= p == NeowRelic::PhialHolster;
                    saw_coffer_other |= p == NeowRelic::LostCoffer;
                }
            }
        }
        assert!(
            saw_sacrifice > 1000,
            "NeowsSacrifice rolled {saw_sacrifice}x"
        );
        assert!(saw_phial_other && saw_coffer_other);
    }

    /// The two v0.111.0 curses map to their own `RelicId`s and names.
    #[test]
    fn v0111_new_curse_relics_present_and_mapped() {
        assert!(CURSE_POOL.contains(&NeowRelic::DowsingRod));
        assert!(CURSE_POOL.contains(&NeowRelic::NeowsSacrifice));
        assert_eq!(
            NeowRelic::DowsingRod.to_relic_id(),
            Some(RelicId::DowsingRod)
        );
        assert_eq!(
            NeowRelic::NeowsSacrifice.to_relic_id(),
            Some(RelicId::NeowsSacrifice)
        );
        assert_eq!(NeowRelic::DowsingRod.snake_case_id(), "DOWSING_ROD");
        assert_eq!(NeowRelic::NeowsSacrifice.snake_case_id(), "NEOWS_SACRIFICE");
    }

    /// The three relics new to the v0.107.1 Neow pools are present + map to
    /// their RelicIds (guards the `to_relic_id` wiring for the additions).
    #[test]
    fn v0107_new_neow_relics_present_and_mapped() {
        assert!(POSITIVE_POOL.contains(&NeowRelic::FishingRod));
        assert!(POSITIVE_POOL.contains(&NeowRelic::Kaleidoscope));
        assert!(CURSE_POOL.contains(&NeowRelic::SilkenTress));
        assert_eq!(
            NeowRelic::FishingRod.to_relic_id(),
            Some(RelicId::FishingRod)
        );
        assert_eq!(
            NeowRelic::Kaleidoscope.to_relic_id(),
            Some(RelicId::Kaleidoscope)
        );
        assert_eq!(
            NeowRelic::SilkenTress.to_relic_id(),
            Some(RelicId::SilkenTress)
        );
    }

    // v0.107.1 RNG rework: anchored to a SCRAPED v0.103.2 human run
    // (System.Random). Under v0.107.1's MegaRandom the same seed yields different
    // Neow offers, so this v0.103.2 corpus anchor cannot hold on this branch (the
    // run corpus is build_id==v0.103.2; no v0.107.1 corpus exists yet).
    #[ignore = "v0.103.2 scraped-corpus anchor; invalid under v0.107.1 MegaRandom RNG (no v0.107.1 corpus yet)"]
    #[test]
    fn corpus_seed_2j2dekxjp9_matches_scraped_offers() {
        // Bead `sts2-rs-k8z6m` corpus-parity anchor. Scraped real-game
        // run at `sts2_run_scraper/data/files/the-ironclad/2026-05/
        // 05d33913-6a09-4a79-aacd-9d1409af18eb.run`:
        //   seed = "2J2DEKXJP9", ancient_choice =
        //   [PHIAL_HOLSTER, LOST_COFFER, LEAFY_POULTICE].
        //
        // The full RunState wiring (NetId offset, RNG salt "NEOW") is
        // exercised via the `RunState::roll_neow_offers_if_needed`
        // path, not just `roll_neow_offers` here, so this test imports
        // the helper from runstate's seed convention. We mirror the
        // canonical-seed → hash pipeline inline to keep this test in
        // sts2-core (no sts2-content dep).
        let canonical = crate::canonicalize_seed("2J2DEKXJP9");
        let master = crate::deterministic_hash_code_old(&canonical);
        // Reconstruct the same Rng the RunState path uses: master+NetId,
        // then mix the "NEOW" salt via with_name. Pins:
        // `EventModel.cs:193`, `NGame.cs:884` (NetId=1uL in SP).
        const NET_ID: i32 = 1;
        let mut rng = Rng::with_name(u64::from((master.wrapping_add(NET_ID)) as u32), "NEOW");
        let offers = roll_neow_offers(&mut rng);
        assert_eq!(offers.positive_1, NeowRelic::PhialHolster);
        assert_eq!(offers.positive_2, NeowRelic::LostCoffer);
        assert_eq!(offers.cursed, NeowRelic::LeafyPoultice);
    }

    #[test]
    fn to_relic_id_maps_w9a_e_variants() {
        // Bead `sts2-rs-4rwv4` (W9a-c) + `sts2-rs-wjuoi` (W9e) +
        // `sts2-rs-w6hdj` (W9d): the last five C#-only Neow relics.
        assert_eq!(
            NeowRelic::PhialHolster.to_relic_id(),
            Some(RelicId::PhialHolster)
        );
        assert_eq!(
            NeowRelic::WingedBoots.to_relic_id(),
            Some(RelicId::WingedBoots)
        );
        assert_eq!(
            NeowRelic::NeowsTalisman.to_relic_id(),
            Some(RelicId::NeowsTalisman)
        );
        assert_eq!(
            NeowRelic::NeowsBones.to_relic_id(),
            Some(RelicId::NeowsBones)
        );
        assert_eq!(
            NeowRelic::HeftyTablet.to_relic_id(),
            Some(RelicId::HeftyTablet)
        );
    }

    #[test]
    fn snake_case_ids_match_relic_id_for_overlapping_variants() {
        // Any NeowRelic with a RelicId counterpart must emit the same
        // SCREAMING_SNAKE_CASE string as the RelicId.
        for nr in [
            NeowRelic::ArcaneScroll,
            NeowRelic::BoomingConch,
            NeowRelic::GoldenPearl,
            NeowRelic::LeadPaperweight,
            NeowRelic::LostCoffer,
            NeowRelic::NeowsTorment,
            NeowRelic::NewLeaf,
            NeowRelic::PreciseScissors,
            NeowRelic::MassiveScroll,
            NeowRelic::LavaRock,
            NeowRelic::SmallCapsule,
            NeowRelic::NutritiousOyster,
            NeowRelic::StoneHumidifier,
            NeowRelic::Pomander,
            NeowRelic::CursedPearl,
            NeowRelic::LargeCapsule,
            NeowRelic::LeafyPoultice,
            NeowRelic::PrecariousShears,
            NeowRelic::SilverCrucible,
            NeowRelic::ScrollBoxes,
            NeowRelic::FishingRod,
            NeowRelic::Kaleidoscope,
            NeowRelic::SilkenTress,
            NeowRelic::DowsingRod,
            NeowRelic::NeowsSacrifice,
        ] {
            let rid = nr.to_relic_id().expect("variant has RelicId");
            assert_eq!(
                nr.snake_case_id(),
                rid.to_python_name(),
                "mismatch for {nr:?}"
            );
        }
    }
}
