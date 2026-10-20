//! `RelicModel.IsAllowed(IRunState)` predicates (RL-C9).
//!
//! C# `RelicGrabBag.RemoveDisallowedRelicsFromDeques` (`RelicGrabBag.cs:217-239`)
//! drops every relic whose `IsAllowed(runState)` is false before each pull.
//! The base implementation returns true (`RelicModel.cs:396-399`); exactly 20
//! relics override it. `IsAllowedAtNeow` delegates to `IsAllowed`
//! (`RelicModel.cs:401-404`) except where `Kaleidoscope`/`ScrollBoxes` add a
//! further gate (not part of the grab bag).
//!
//! The predicates read three facts of the run, gathered in [`RelicAllowCtx`] so
//! each can be unit-tested against its C# line without a full `RunState`.

use crate::relics::RelicId;

/// `RelicModel.IsBeforeAct3TreasureChest` floor limit in a solo run
/// (`RelicModel.cs:407-411`: `Players.Count > 1 ? 38 : 41`).
pub const BEFORE_ACT3_CHEST_FLOOR_SOLO: i32 = 41;
/// Same limit with more than one player.
pub const BEFORE_ACT3_CHEST_FLOOR_MULTI: i32 = 38;

/// `UnlockState.NumberOfRuns` the oracle save reports (`number_of_runs` in the
/// c1 dry save), i.e. "not a first run".
pub const ORACLE_NUMBER_OF_RUNS: i32 = 999_999_999;

/// The run facts `IsAllowed` overrides read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelicAllowCtx {
    /// `runState.TotalFloor` (`RunState.cs:99`, map point history count).
    pub total_floor: i32,
    /// `runState.Players.Count`.
    pub player_count: usize,
    /// Any player is an `Ironclad` whose `UnlockState.NumberOfRuns == 0`
    /// (`LastingCandy.cs:84-95`).
    pub ironclad_first_run: bool,
}

impl RelicAllowCtx {
    /// `IsBeforeAct3TreasureChest` (`RelicModel.cs:407-411`).
    #[must_use]
    pub fn is_before_act3_treasure_chest(&self) -> bool {
        let limit = if self.player_count > 1 {
            BEFORE_ACT3_CHEST_FLOOR_MULTI
        } else {
            BEFORE_ACT3_CHEST_FLOOR_SOLO
        };
        self.total_floor < limit
    }
}

/// `relic.IsAllowed(runState)`; each arm cites its C# override
/// (`MegaCrit.Sts2.Core.Models.Relics/<Name>.cs:<line>`).
#[must_use]
pub fn relic_is_allowed(id: RelicId, ctx: &RelicAllowCtx) -> bool {
    match id {
        // `return RelicModel.IsBeforeAct3TreasureChest(runState);`
        RelicId::AmethystAubergine // AmethystAubergine.cs:20
        | RelicId::BookOfFiveRings // BookOfFiveRings.cs:72
        | RelicId::BowlerHat // BowlerHat.cs:20
        | RelicId::DragonFruit // DragonFruit.cs:17
        | RelicId::FrozenEgg // FrozenEgg.cs:16
        | RelicId::Girya // Girya.cs:45
        | RelicId::JuzuBracelet // JuzuBracelet.cs:12
        | RelicId::LuckyFysh // LuckyFysh.cs:19
        | RelicId::MealTicket // MealTicket.cs:17
        | RelicId::MoltenEgg // MoltenEgg.cs:16
        | RelicId::OldCoin // OldCoin.cs:20
        | RelicId::Planisphere // Planisphere.cs:18
        | RelicId::Shovel // Shovel.cs:13
        | RelicId::ToxicEgg // ToxicEgg.cs:16
        | RelicId::WhiteBeastStatue // WhiteBeastStatue.cs:12
        | RelicId::WhiteStar => ctx.is_before_act3_treasure_chest(), // WhiteStar.cs:14
        // LastingCandy.cs:82-98: no Ironclad on a first run, then the floor limit.
        RelicId::LastingCandy => !ctx.ironclad_first_run && ctx.is_before_act3_treasure_chest(),
        // MassiveScroll.cs:19-22: `runState.Players.Count > 1`.
        RelicId::MassiveScroll => ctx.player_count > 1,
        // SilverCrucible.cs:81-84, WingedBoots.cs:47-50: `Players.Count == 1`.
        RelicId::SilverCrucible | RelicId::WingedBoots => ctx.player_count == 1,
        // `RelicModel.IsAllowed` base (`RelicModel.cs:396-399`).
        _ => true,
    }
}

/// The 20 relics that override `IsAllowed` (inventory RL-C9), alphabetical.
pub const IS_ALLOWED_OVERRIDES: [RelicId; 20] = [
    RelicId::AmethystAubergine,
    RelicId::BookOfFiveRings,
    RelicId::BowlerHat,
    RelicId::DragonFruit,
    RelicId::FrozenEgg,
    RelicId::Girya,
    RelicId::JuzuBracelet,
    RelicId::LastingCandy,
    RelicId::LuckyFysh,
    RelicId::MassiveScroll,
    RelicId::MealTicket,
    RelicId::MoltenEgg,
    RelicId::OldCoin,
    RelicId::Planisphere,
    RelicId::Shovel,
    RelicId::SilverCrucible,
    RelicId::ToxicEgg,
    RelicId::WhiteBeastStatue,
    RelicId::WhiteStar,
    RelicId::WingedBoots,
];

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(total_floor: i32, player_count: usize, first: bool) -> RelicAllowCtx {
        RelicAllowCtx {
            total_floor,
            player_count,
            ironclad_first_run: first,
        }
    }

    const FLOOR_GATED: [RelicId; 16] = [
        RelicId::AmethystAubergine,
        RelicId::BookOfFiveRings,
        RelicId::BowlerHat,
        RelicId::DragonFruit,
        RelicId::FrozenEgg,
        RelicId::Girya,
        RelicId::JuzuBracelet,
        RelicId::LuckyFysh,
        RelicId::MealTicket,
        RelicId::MoltenEgg,
        RelicId::OldCoin,
        RelicId::Planisphere,
        RelicId::Shovel,
        RelicId::ToxicEgg,
        RelicId::WhiteBeastStatue,
        RelicId::WhiteStar,
    ];

    /// Each of the 16 `IsBeforeAct3TreasureChest` relics: allowed below floor
    /// 41 (solo) / 38 (multi), disallowed at the limit (`RelicModel.cs:409`).
    #[test]
    fn floor_gated_relics_follow_is_before_act3_treasure_chest() {
        for id in FLOOR_GATED {
            assert!(relic_is_allowed(id, &ctx(0, 1, false)), "{id:?} floor 0");
            assert!(relic_is_allowed(id, &ctx(40, 1, false)), "{id:?} floor 40");
            assert!(!relic_is_allowed(id, &ctx(41, 1, false)), "{id:?} floor 41");
            assert!(relic_is_allowed(id, &ctx(37, 2, false)), "{id:?} mp 37");
            assert!(!relic_is_allowed(id, &ctx(38, 2, false)), "{id:?} mp 38");
        }
    }

    /// `LastingCandy.cs:84-95`: blocked for an Ironclad first run at any floor,
    /// otherwise the same floor limit.
    #[test]
    fn lasting_candy_blocks_first_ironclad_run_and_late_floors() {
        assert!(relic_is_allowed(RelicId::LastingCandy, &ctx(10, 1, false)));
        assert!(!relic_is_allowed(RelicId::LastingCandy, &ctx(10, 1, true)));
        assert!(!relic_is_allowed(RelicId::LastingCandy, &ctx(41, 1, false)));
        assert!(relic_is_allowed(RelicId::LastingCandy, &ctx(37, 2, false)));
        assert!(!relic_is_allowed(RelicId::LastingCandy, &ctx(38, 2, false)));
    }

    /// `MassiveScroll.cs:21`: `Players.Count > 1`.
    #[test]
    fn massive_scroll_is_multiplayer_only() {
        assert!(!relic_is_allowed(RelicId::MassiveScroll, &ctx(0, 1, false)));
        assert!(relic_is_allowed(RelicId::MassiveScroll, &ctx(0, 2, false)));
        assert!(relic_is_allowed(RelicId::MassiveScroll, &ctx(99, 2, false)));
    }

    /// `SilverCrucible.cs:83`, `WingedBoots.cs:49`: `Players.Count == 1`, no
    /// floor limit.
    #[test]
    fn silver_crucible_and_winged_boots_are_solo_only() {
        for id in [RelicId::SilverCrucible, RelicId::WingedBoots] {
            assert!(relic_is_allowed(id, &ctx(0, 1, false)), "{id:?}");
            assert!(
                relic_is_allowed(id, &ctx(99, 1, false)),
                "{id:?} late floor"
            );
            assert!(!relic_is_allowed(id, &ctx(0, 2, false)), "{id:?} mp");
        }
    }

    /// Everything that does not override `IsAllowed` is always allowed
    /// (`RelicModel.cs:396-399`), and the override list is the 20 named in
    /// the inventory with no duplicates.
    #[test]
    fn non_overrides_are_always_allowed_and_override_list_is_20() {
        let mut seen = std::collections::BTreeSet::new();
        for id in IS_ALLOWED_OVERRIDES {
            assert!(seen.insert(format!("{id:?}")), "duplicate {id:?}");
        }
        assert_eq!(seen.len(), 20);
        for id in [RelicId::Anchor, RelicId::BurningBlood, RelicId::Circlet] {
            assert!(relic_is_allowed(id, &ctx(99, 3, true)), "{id:?}");
        }
    }
}
