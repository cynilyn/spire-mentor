//! R1 (CR): `Hook.ModifyRewards` on a combat room's reward set (`Hook.cs:1584-1600`, populate order
//! `RewardsSet.cs:131-149`). The engine used to ignore every relic's `TryModifyRewards` except BlackStar's, so a
//! PrayerWheel / WhiteStar / AmethystAubergine / LavaRock run drifted off the oracle's `Rewards` stream at the first
//! affected room (R1 lockstep: P0A59, P0A65, P0A58, P0A75; golden RTGG_H_101 i=292).
//!
//! Paired witnesses: v0.107.1 keeps its pre-R1 behaviour (nothing is added), v0.111.0 adds the rewards. Expected values
//! come from the C# sources and the oracle (f79d3a9): a relic-appended 3-card `CardReward` populates
//! `3 x (rarity, pick, upgrade)` = 9 `PlayerRng.Rewards` reads (oracle counter differences +9 in all three seeds), and
//! `AmethystAubergine` adds `GoldReward(15)` (`AmethystAubergine.cs:18,36`, no read).

use sts2_content::events::dispatch;
use sts2_core::run_manager::{ActionResult, Phase, RunAction, RunManager};
use sts2_core::{EventId, GameVersion, RelicId};

const VERSIONS: [GameVersion; 2] = [GameVersion::V0_107_1, GameVersion::V0_111_0];
/// Reads of `PlayerRng.Rewards` one 3-card reward costs in act 1 (oracle-measured, see module doc).
const READS_PER_CARD_REWARD: u32 = 9;

fn run_in_event(event: EventId, version: GameVersion, relics: &[RelicId]) -> RunManager {
    sts2_content::relics::dispatch::ensure_installed();
    let mut rm = sts2_content::run_callbacks::new_run_manager_for_character(
        7,
        sts2_core::CharacterId::Ironclad,
    )
    .with_game_version(version);
    sts2_content::run_callbacks::initialize_run_setup(&mut rm);
    let mut rm = rm.with_neow_start();
    rm.run_state.direct_event_relic_grant = true;
    rm.run_state.enter_event_room(event);
    assert_eq!(rm.phase(), Phase::Event);
    for r in relics {
        rm.run_state.relics.push(*r);
    }
    rm
}

fn choose(rm: &mut RunManager, option_id: &str) {
    let opts = dispatch::present(&rm.run_state).expect("event active");
    let idx = opts
        .iter()
        .position(|o| o.option_id == option_id)
        .unwrap_or_else(|| panic!("option {option_id} not offered"));
    assert_eq!(
        rm.take_action(RunAction::EventChoice { idx }),
        ActionResult::Advanced
    );
}

/// Win the Monster-room fight `TheLanternKey` stages and return the run at the first card reward.
fn win_monster_fight(version: GameVersion, relics: &[RelicId]) -> RunManager {
    let mut rm = run_in_event(EventId::TheLanternKey, version, relics);
    choose(&mut rm, "keep_key");
    choose(&mut rm, "fight");
    assert_eq!(rm.phase(), Phase::Combat);
    assert_eq!(rm.resolve_combat_fast(0.0, true), ActionResult::Advanced);
    assert_eq!(rm.phase(), Phase::CardReward);
    rm
}

fn skip(rm: &mut RunManager) {
    assert_eq!(
        rm.take_action(RunAction::PickCardReward { idx: None }),
        ActionResult::Advanced
    );
}

/// `PrayerWheel.TryModifyRewards` (Monster room): an extra `CardReward(ForRoom(Monster), 3)`.
#[test]
fn prayer_wheel_adds_a_second_card_reward_on_v0111_only() {
    for version in VERSIONS {
        let mut plain = win_monster_fight(version, &[]);
        let mut wheel = win_monster_fight(version, &[RelicId::PrayerWheel]);
        assert_eq!(plain.card_reward_options().len(), 3);
        skip(&mut plain);
        assert_eq!(plain.phase(), Phase::MapChoice, "{version:?}: one reward");
        skip(&mut wheel);
        if version >= GameVersion::V0_111_0 {
            assert_eq!(wheel.phase(), Phase::CardReward, "second card screen");
            assert_eq!(wheel.card_reward_options().len(), 3);
            skip(&mut wheel);
        }
        assert_eq!(wheel.phase(), Phase::MapChoice, "{version:?}");
        let extra = wheel.run_state.rng.rewards.counter() - plain.run_state.rng.rewards.counter();
        let want = if version >= GameVersion::V0_111_0 {
            READS_PER_CARD_REWARD
        } else {
            0
        };
        assert_eq!(
            extra, want,
            "{version:?}: Rewards reads of the extra reward"
        );
    }
}

/// `AmethystAubergine.TryModifyRewards` (any combat room): `GoldReward(15)`, no `Rewards` read. Both builds grant it
/// (the pre-R1 callback for v0.107.1, the run manager's ModifyRewards walk for v0.111.0); the witness is that moving the
/// v0.111.0 grant into that walk neither drops nor doubles it.
#[test]
fn amethyst_aubergine_gold_is_granted_once_on_both_builds() {
    for version in VERSIONS {
        let plain = win_monster_fight(version, &[]);
        let amethyst = win_monster_fight(version, &[RelicId::AmethystAubergine]);
        let gained = amethyst.run_state.gold - plain.run_state.gold;
        assert_eq!(gained, 15, "{version:?}");
        assert_eq!(
            amethyst.run_state.rng.rewards.counter(),
            plain.run_state.rng.rewards.counter(),
            "{version:?}: fixed gold reads no RNG"
        );
    }
}

/// The relic-appended reward is rolled after the room's own rewards and BEFORE any later room's: the stream position
/// after the whole room equals the plain run's plus exactly one card reward, so the next room's gold roll is the one
/// the oracle sees (RTGG_H_101: +14 vs +12 after the elite room).
#[test]
fn rewards_stream_after_the_room_is_shifted_by_exactly_the_extra_reward() {
    let plain = win_monster_fight(GameVersion::V0_111_0, &[]);
    let mut wheel = win_monster_fight(GameVersion::V0_111_0, &[RelicId::PrayerWheel]);
    let before = wheel.run_state.rng.rewards.counter();
    // Reading `Rewards` for the first screen is identical; the extra reward is already populated (rolled) before it opens.
    assert_eq!(
        before,
        plain.run_state.rng.rewards.counter() + READS_PER_CARD_REWARD
    );
    skip(&mut wheel);
    skip(&mut wheel);
    assert_eq!(
        wheel.run_state.rng.rewards.counter(),
        before,
        "picking reads nothing more"
    );
}
