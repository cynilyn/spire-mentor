//! Unknown (`?`) map-point room-type resolver — bead `sts2-rs-khzyk`
//! (P9.2.f bounded slice).
//!
//! Ports `sts2-rl-agent@01c094d:sts2_env/run/odds.py::UnknownMapPointOdds`
//! (the rolling-odds pity-counter for `?` rooms) plus the resolver
//! that drives it (`Player.resolve_unknown_room` —
//! `sts2_env/run/run_state.py`, called by
//! `run_manager._enter_room`).
//!
//! ## What this module lands
//!
//! - [`UnknownMapPointOdds`] — the rolling per-room-type odds with
//!   reset-on-roll / grow-on-skip semantics. Boost path (Elite via the
//!   `modify_odds_increase_for_unrolled_room_type` modifier chain) is
//!   deferred — the modifier list isn't ported yet, and Elite starts
//!   at the Python `BASE_ELITE = -1.00` (impossible until boosted) so
//!   the base resolver path never returns Elite anyway.
//! - [`RunState::resolve_unknown_room`] — runs the modify-room-types
//!   channel (bead `sts2-rs-vuxmm`, JuzuBracelet's downstream consumer)
//!   then rolls the odds to produce a concrete [`RoomType`].
//!
//! ## What this module deliberately skips (parity-grade-but-bounded)
//!
//! - **Per-card `modify_unknown_map_point_room_types`** — Python walks
//!   `player.deck` cards before relics. No card today has that hook in
//!   the Rust port; relic-side is the only listener that matters at
//!   the pin.
//! - **`modify_odds_increase_for_unrolled_room_type` modifier chain**
//!   — Python's `RunState.modifiers: list[Modifier]` isn't ported.
//!   This affects only the per-roll growth rate (a 1-2 relic interaction
//!   today); the unmodified growth rate is the parity-default.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use crate::dotnet_rng::Rng;
use crate::runstate::{RoomType, RunState};

// The odds are `f32`, like C# `UnknownMapPointOdds` (`Dictionary<RoomType,
// float>`): the roll is compared against an `f32` running sum, and the
// per-skip growth is an `f32` add, so `f64` bookkeeping drifts from the game
// at the boundary (RG-46).

/// Python `BASE_MONSTER = 0.10`. Probability `?` rolls a monster fight
/// on first visit; grows by this much per skipped roll.
pub const BASE_MONSTER: f32 = 0.10;
/// Python `BASE_ELITE = -1.00`. Sentinel meaning "impossible until a
/// modifier boosts the odds positive". Today no modifier path lands an
/// Elite via `?` — the field is reserved for parity.
pub const BASE_ELITE: f32 = -1.00;
/// Python `BASE_TREASURE = 0.02`. Probability `?` rolls a treasure
/// chest on first visit.
pub const BASE_TREASURE: f32 = 0.02;
/// Python `BASE_SHOP = 0.03`. Probability `?` rolls a merchant on
/// first visit.
pub const BASE_SHOP: f32 = 0.03;

/// C# room-type ordering used by the fallback selector when
/// `RoomType::Event` is excluded from the candidate set. Lower number
/// = preferred fallback. Mirrors
/// `C_SHARP_ROOM_TYPE_ORDER` in `sts2_env/run/odds.py`.
#[must_use]
pub fn fallback_order(rt: RoomType) -> u8 {
    match rt {
        RoomType::Monster => 1,
        RoomType::Elite => 2,
        RoomType::Boss => 3,
        RoomType::Treasure => 4,
        RoomType::Shop => 5,
        RoomType::Event => 6,
        RoomType::RestSite => 7,
        RoomType::Unknown => u8::MAX,
    }
}

/// Rolling pity-odds container for `?` rooms. One per
/// [`crate::RunState`]. Mirrors Python
/// `sts2_env/run/odds.py::UnknownMapPointOdds`.
///
/// `current` holds the live per-type odds; rolled type → reset to
/// `base`; un-rolled types in the candidate set → grow by `base`.
#[derive(Clone, Debug)]
// Custom `PartialEq` / `Eq` on bit patterns: odds values are built
// from named constants only (no NaN reachable) and mutated by adding
// those same constants, so bitwise comparison is total and lets the
// containing `RunState` keep its derived `Eq`.
pub struct UnknownMapPointOdds {
    /// Live odds per room type — the value compared against the roll.
    pub current: BTreeMap<RoomType, f32>,
    /// Reset / growth base — what each `current[type]` snaps back to
    /// on roll and grows by on skip.
    base: BTreeMap<RoomType, f32>,
}

impl Default for UnknownMapPointOdds {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialEq for UnknownMapPointOdds {
    fn eq(&self, other: &Self) -> bool {
        fn bits_match(a: &BTreeMap<RoomType, f32>, b: &BTreeMap<RoomType, f32>) -> bool {
            if a.len() != b.len() {
                return false;
            }
            a.iter()
                .zip(b.iter())
                .all(|((ka, va), (kb, vb))| ka == kb && va.to_bits() == vb.to_bits())
        }
        bits_match(&self.current, &other.current) && bits_match(&self.base, &other.base)
    }
}

impl Eq for UnknownMapPointOdds {}

impl UnknownMapPointOdds {
    /// Construct a fresh roller with Python's base odds.
    #[must_use]
    pub fn new() -> Self {
        let mut current = BTreeMap::new();
        let mut base = BTreeMap::new();
        for (rt, odds) in [
            (RoomType::Monster, BASE_MONSTER),
            (RoomType::Elite, BASE_ELITE),
            (RoomType::Treasure, BASE_TREASURE),
            (RoomType::Shop, BASE_SHOP),
        ] {
            current.insert(rt, odds);
            base.insert(rt, odds);
        }
        Self { current, base }
    }

    /// Reset all live odds to their base values. Mirrors Python
    /// `UnknownMapPointOdds.reset_to_base()`. Used by tests + the
    /// boss-transition restart path.
    pub fn reset_to_base(&mut self) {
        for (rt, odds) in &self.base {
            self.current.insert(*rt, *odds);
        }
    }

    /// Roll a concrete [`RoomType`] for an `?` room.
    ///
    /// `room_types` is the candidate set after channel-modifications
    /// (see [`RunState::resolve_unknown_room`]). It MUST include
    /// `RoomType::Event` as the default fallback per Python — if no
    /// roll lands, Event is returned.
    ///
    /// Mirrors C# `UnknownMapPointOdds.Roll` from the `NextFloat` read on,
    /// minus the per-modifier `ModifyOddsIncreaseForUnrolledRoomType` chain.
    /// The first-run early-out (`NumberOfRuns == 0`) precedes it and lives in
    /// [`RunState::resolve_unknown_room`], which has the run state it needs.
    pub fn roll(&mut self, rng: &mut Rng, room_types: &BTreeSet<RoomType>) -> RoomType {
        // C# `float num2 = _rng.NextFloat()`: the f64 sample narrowed to f32
        // (`Rng.cs:126`). Still exactly one read of the stream.
        let roll_val = rng.next_float(1.0) as f32;
        self.roll_with_value(roll_val, room_types)
    }

    /// [`Self::roll`] after the `NextFloat` read, so a test can pin the exact
    /// `f32` the cumulative walk is compared against.
    pub fn roll_with_value(&mut self, roll_val: f32, room_types: &BTreeSet<RoomType>) -> RoomType {
        // Default fallback. Python: if EVENT in room_types → result =
        // EVENT, else result = min(room_types, key=C# order).
        let mut result = if room_types.contains(&RoomType::Event) {
            RoomType::Event
        } else {
            *room_types
                .iter()
                .min_by_key(|rt| fallback_order(**rt))
                .expect("room_types must be non-empty")
        };

        // Cumulative roll over odds[type] for type in room_types where
        // odds >= 0. Python iterates `self._current.items()` (insertion
        // order on a dict). BTreeMap is `RoomType`-ordered (engine
        // enum order: Unknown < Monster < Elite < Boss < Shop <
        // RestSite < Treasure < Event), which differs from Python's
        // insertion order (Monster, Elite, Treasure, Shop). Match
        // Python exactly by iterating in insertion order.
        let mut cumulative = 0.0_f32;
        for rt in [
            RoomType::Monster,
            RoomType::Elite,
            RoomType::Treasure,
            RoomType::Shop,
        ] {
            if !room_types.contains(&rt) {
                continue;
            }
            let odds = *self.current.get(&rt).unwrap_or(&0.0);
            if odds < 0.0 {
                continue;
            }
            // f32 running sum, as in C# (`num3 += num4`, both `float`).
            cumulative += odds;
            if roll_val <= cumulative {
                result = rt;
                break;
            }
        }

        // Update odds: rolled type → reset to base; un-rolled types
        // in the candidate set → grow by base. Python applies the
        // modifier chain to the growth term; we use base_odds
        // unmodified (see module docs).
        for (rt, base_odds) in self.base.clone() {
            if rt == result {
                self.current.insert(rt, base_odds);
            } else if room_types.contains(&rt) {
                let cur = *self.current.get(&rt).unwrap_or(&0.0);
                self.current.insert(rt, cur + base_odds);
            }
        }

        result
    }
}

impl RunState {
    /// Resolve an `?` (unknown) map-point room type to a concrete
    /// [`RoomType`]. Mirrors the resolver in Python
    /// `run_manager._enter_room` (called when the player lands on an
    /// `?`).
    ///
    /// Steps:
    /// 0. First run (`number_of_runs == 0`): the first two `?` are Events and
    ///    the third a Monster, with no RNG read and no odds update
    ///    (`UnknownMapPointOdds.cs:99-112`). The count is the `?` map points
    ///    already in `map_point_history`.
    /// 1. Build the candidate set
    ///    `{Monster, Elite, Treasure, Shop, Event} − blacklist`
    ///    (`_nonEventOdds.Keys.Append(Event).Except(blacklist)`). Elite is in
    ///    the set but never rolls: its odds start at -1 and stay negative
    ///    (no modifier-boost path lands today; see module docs).
    /// 2. Walk
    ///    [`crate::run_effects::dispatch_modify_unknown_map_point_room_types`]
    ///    so JuzuBracelet (and any future listener) can drop
    ///    candidates. Channel landed in bead `sts2-rs-vuxmm`.
    /// 3. Roll via [`Self::unknown_map_point_odds`] over the live
    ///    `unknown_map_point` named RNG stream
    ///    (`crate::run_rng_set::RunRngSet::unknown_map_point`).
    ///
    /// Bead `sts2-rs-khzyk` (P9.2.f bounded slice).
    pub fn resolve_unknown_room(&mut self, blacklist: BTreeSet<RoomType>) -> RoomType {
        // First-run tutorial sequence: no RNG, no odds update.
        if self.number_of_runs == 0 {
            let seen =
                self.count_map_point_history_entries(None, Some(crate::map::MapPointType::Unknown));
            if seen < 2 {
                return RoomType::Event;
            }
            if seen == 2 {
                return RoomType::Monster;
            }
        }

        // Default candidate set: every `_nonEventOdds` key (Monster, Elite,
        // Treasure, Shop) plus Event, minus blacklist. Elite's odds are
        // negative so it never rolls, but it is a member of the set the
        // `modify_unknown_map_point_room_types` listeners see.
        let mut room_types: BTreeSet<RoomType> = [
            RoomType::Monster,
            RoomType::Elite,
            RoomType::Treasure,
            RoomType::Shop,
            RoomType::Event,
        ]
        .into_iter()
        .filter(|rt| !blacklist.contains(rt))
        .collect();

        // Walk the relic-side channel (JuzuBracelet drops `Monster`
        // pre-act-3-treasure, etc.).
        room_types = self.modify_unknown_map_point_room_types(room_types);

        // Roll.
        self.unknown_map_point_odds
            .roll(&mut self.rng.unknown_map_point, &room_types)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run_rng_set::RunRngSet;

    fn rng(seed: i32) -> Rng {
        let mut set = RunRngSet::from_run_seed(u64::from(seed as u32));
        std::mem::replace(&mut set.unknown_map_point, Rng::with_name(0, "scratch"))
    }

    #[test]
    fn new_starts_at_python_base_odds() {
        let odds = UnknownMapPointOdds::new();
        assert!((odds.current[&RoomType::Monster] - BASE_MONSTER).abs() < 1e-12);
        assert!((odds.current[&RoomType::Elite] - BASE_ELITE).abs() < 1e-12);
        assert!((odds.current[&RoomType::Treasure] - BASE_TREASURE).abs() < 1e-12);
        assert!((odds.current[&RoomType::Shop] - BASE_SHOP).abs() < 1e-12);
    }

    #[test]
    fn elite_negative_base_never_rolled_without_boost() {
        let mut odds = UnknownMapPointOdds::new();
        let candidates: BTreeSet<RoomType> = [
            RoomType::Monster,
            RoomType::Elite,
            RoomType::Treasure,
            RoomType::Shop,
            RoomType::Event,
        ]
        .into_iter()
        .collect();
        let mut r = rng(7);
        for _ in 0..200 {
            let got = odds.roll(&mut r, &candidates);
            assert_ne!(got, RoomType::Elite, "Elite must not roll at base odds");
        }
    }

    #[test]
    fn rolled_type_resets_to_base_skipped_types_grow() {
        let mut odds = UnknownMapPointOdds::new();
        // Force a monster roll: crank monster odds so any roll < 1
        // hits monster.
        odds.current.insert(RoomType::Monster, 1.0);
        let candidates: BTreeSet<RoomType> = [
            RoomType::Monster,
            RoomType::Treasure,
            RoomType::Shop,
            RoomType::Event,
        ]
        .into_iter()
        .collect();
        let mut r = rng(11);
        let got = odds.roll(&mut r, &candidates);
        assert_eq!(got, RoomType::Monster);
        // Monster reset to base.
        assert!((odds.current[&RoomType::Monster] - BASE_MONSTER).abs() < 1e-12);
        // Treasure / Shop grew by their base.
        assert!(odds.current[&RoomType::Treasure] > BASE_TREASURE);
        assert!(odds.current[&RoomType::Shop] > BASE_SHOP);
    }

    #[test]
    fn event_is_fallback_when_no_odds_land() {
        let mut odds = UnknownMapPointOdds::new();
        // Wipe positive odds so cumulative never exceeds the roll.
        odds.current.insert(RoomType::Monster, 0.0);
        odds.current.insert(RoomType::Treasure, 0.0);
        odds.current.insert(RoomType::Shop, 0.0);
        let candidates: BTreeSet<RoomType> = [
            RoomType::Monster,
            RoomType::Treasure,
            RoomType::Shop,
            RoomType::Event,
        ]
        .into_iter()
        .collect();
        let mut r = rng(13);
        let got = odds.roll(&mut r, &candidates);
        assert_eq!(got, RoomType::Event);
    }

    #[test]
    fn event_excluded_falls_back_to_c_sharp_min_order() {
        let mut odds = UnknownMapPointOdds::new();
        odds.current.insert(RoomType::Monster, 0.0);
        odds.current.insert(RoomType::Treasure, 0.0);
        odds.current.insert(RoomType::Shop, 0.0);
        // Drop Event; the C# order (Monster=1 < Treasure=4 < Shop=5)
        // makes Monster the fallback.
        let candidates: BTreeSet<RoomType> =
            [RoomType::Monster, RoomType::Treasure, RoomType::Shop]
                .into_iter()
                .collect();
        let mut r = rng(17);
        let got = odds.roll(&mut r, &candidates);
        assert_eq!(got, RoomType::Monster);
    }

    #[test]
    fn reset_to_base_restores_every_room_type() {
        let mut odds = UnknownMapPointOdds::new();
        odds.current.insert(RoomType::Monster, 0.99);
        odds.current.insert(RoomType::Treasure, 0.99);
        odds.reset_to_base();
        assert!((odds.current[&RoomType::Monster] - BASE_MONSTER).abs() < 1e-12);
        assert!((odds.current[&RoomType::Treasure] - BASE_TREASURE).abs() < 1e-12);
        assert!((odds.current[&RoomType::Shop] - BASE_SHOP).abs() < 1e-12);
    }

    /// RG-46 (`UnknownMapPointOdds.cs:121-131`): the cumulative walk is an
    /// `f32` running sum. A roll equal to that f32 sum must land on the type
    /// that completes it; an f64 sum of the same operands would not match.
    #[test]
    fn cumulative_sum_is_f32() {
        let candidates: BTreeSet<RoomType> = [
            RoomType::Monster,
            RoomType::Treasure,
            RoomType::Shop,
            RoomType::Event,
        ]
        .into_iter()
        .collect();
        // Monster 0.1f + Treasure 0.02f, summed in f32.
        let sum_mt: f32 = BASE_MONSTER + BASE_TREASURE;
        let mut odds = UnknownMapPointOdds::new();
        assert_eq!(
            odds.roll_with_value(sum_mt, &candidates),
            RoomType::Treasure,
            "roll == f32(monster+treasure) is `<=` the running sum"
        );
        // The f64 sum of the widened operands differs from the f32 sum, which
        // is what an f64 implementation would have compared against.
        let sum_f64 = f64::from(BASE_MONSTER) + f64::from(BASE_TREASURE);
        assert_ne!(f64::from(sum_mt).to_bits(), sum_f64.to_bits());
        // Next representable f32 above the sum falls through to Shop.
        let mut odds = UnknownMapPointOdds::new();
        let above = f32::from_bits(sum_mt.to_bits() + 1);
        assert_eq!(odds.roll_with_value(above, &candidates), RoomType::Shop);
    }

    /// Growth is an f32 add: 0.02f + 0.02f + 0.02f in f32, bit for bit.
    #[test]
    fn growth_is_f32_add() {
        let mut odds = UnknownMapPointOdds::new();
        let candidates: BTreeSet<RoomType> = [RoomType::Monster, RoomType::Treasure]
            .into_iter()
            .collect();
        for _ in 0..3 {
            // roll 2.0 never lands -> fallback min order (Monster) is the
            // result, so Monster resets and Treasure grows.
            odds.roll_with_value(2.0, &candidates);
        }
        let mut expect = BASE_TREASURE;
        for _ in 0..3 {
            expect += BASE_TREASURE;
        }
        assert_eq!(
            odds.current[&RoomType::Treasure].to_bits(),
            expect.to_bits()
        );
    }

    /// RG-47 (`UnknownMapPointOdds.cs:99-112`): with `number_of_runs == 0`
    /// the first two `?` are Events, the third a Monster, none touching the
    /// stream or the odds; the fourth rolls normally.
    #[test]
    fn first_run_unknown_sequence() {
        use crate::map::MapPointType;
        let mut rs = RunState {
            number_of_runs: 0,
            ..RunState::default()
        };
        let before_counter = rs.rng.unknown_map_point.counter();
        let odds_before = rs.unknown_map_point_odds.clone();
        let mut seq = Vec::new();
        for _ in 0..3 {
            let got = rs.resolve_unknown_room(BTreeSet::new());
            seq.push(got);
            // The caller records the visit after the roll.
            rs.append_to_map_point_history(MapPointType::Unknown, got);
        }
        assert_eq!(seq, [RoomType::Event, RoomType::Event, RoomType::Monster]);
        assert_eq!(rs.rng.unknown_map_point.counter(), before_counter);
        assert_eq!(rs.unknown_map_point_odds, odds_before);
        // Fourth `?` is a normal roll: exactly one stream read.
        let _ = rs.resolve_unknown_room(BTreeSet::new());
        assert_eq!(rs.rng.unknown_map_point.counter(), before_counter + 1);
    }

    /// Non-`?` history entries do not advance the first-run count, and a
    /// returning profile (default `number_of_runs`) rolls normally from the
    /// first `?` (one read).
    #[test]
    fn first_run_rule_counts_only_unknown_points_and_is_off_by_default() {
        use crate::map::MapPointType;
        let mut rs = RunState {
            number_of_runs: 0,
            ..RunState::default()
        };
        rs.append_to_map_point_history(MapPointType::Monster, RoomType::Monster);
        rs.append_to_map_point_history(MapPointType::Unknown, RoomType::Event);
        assert_eq!(rs.resolve_unknown_room(BTreeSet::new()), RoomType::Event);
        assert_eq!(rs.rng.unknown_map_point.counter(), 0);

        let mut rs = RunState::default();
        assert_ne!(rs.number_of_runs, 0, "default is a returning profile");
        let _ = rs.resolve_unknown_room(BTreeSet::new());
        assert_eq!(rs.rng.unknown_map_point.counter(), 1);
    }

    /// RG-45: Elite is a member of the candidate set (`_nonEventOdds.Keys`),
    /// so a non-rolled Elite takes the skip-growth step (`-1 + -1`) while
    /// staying un-rollable.
    #[test]
    fn elite_is_in_resolver_candidate_set() {
        let mut rs = RunState::default();
        let got = rs.resolve_unknown_room(BTreeSet::new());
        assert_ne!(got, RoomType::Elite);
        assert_eq!(
            rs.unknown_map_point_odds.current[&RoomType::Elite].to_bits(),
            (-2.0_f32).to_bits()
        );
    }

    #[test]
    fn resolve_unknown_room_honors_blacklist() {
        let mut rs = RunState::default();
        // Push monster odds high so non-blacklisted monster would
        // otherwise win.
        rs.unknown_map_point_odds
            .current
            .insert(RoomType::Monster, 1.0);
        let blacklist: BTreeSet<RoomType> = [RoomType::Monster].into_iter().collect();
        let got = rs.resolve_unknown_room(blacklist);
        assert_ne!(got, RoomType::Monster);
    }
}
