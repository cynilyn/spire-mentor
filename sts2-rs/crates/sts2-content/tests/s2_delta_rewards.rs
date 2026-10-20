//! Stage 2 bundle delta (reward flow) regression tests, pinned to the
//! v0.111.0 decompile (`sts2-wiki/decompiled/v0.111.0/`):
//!
//! - RL-C13: `Hook.ModifyRewards` (`Hook.cs:1584-1600`) — normal pass over
//!   `IterateHookListeners`, then a fresh Late pass; listener order = the
//!   owner's relic acquisition order; listener-appended rewards are populated
//!   (their `Rewards` draws consumed) AFTER the room's own rewards, in list
//!   order (`RewardsSet.cs:93-116`), then collected sorted by `RewardsSetIndex`
//!   (gold 1 < potion 2 < relic 3 < card 5).
//! - RL-B10: Driftwood (`Driftwood.cs:14-22` sets `CardReward.CanReroll` in the
//!   Late pass) and `CardReward.Reroll` (`CardReward.cs:294-304`).
//!
//! Method: the expected values are NOT read back from the code under test. A
//! baseline run (same seed, no listener relics) is advanced by hand through the
//! C# populate order with public primitives, and the live run (listener relics
//! owned) must land on exactly the same stream cursor / relics / gold / cards.

use sts2_content::run_callbacks::{
    generate_card_reward_options, initialize_run_setup, new_run_manager_for_character,
};
use sts2_core::cards::CardId;
use sts2_core::potions::CharacterId;
use sts2_core::run_effects::dispatch_modify_rewards;
use sts2_core::runstate::{CardReward, CardRewardGenCtx, Reward, Room, RoomType, RunState};
use sts2_core::{ActionResult, Phase, RelicId, RunAction};

fn install() {
    sts2_content::cards::dispatch::ensure_installed();
    sts2_content::relics::dispatch::ensure_installed();
    sts2_content::potions::dispatch::ensure_installed();
    sts2_core::relic_effects::ensure_installed();
}

/// A fresh Ironclad run driven (first legal action each step) to the moment a
/// map move has just entered its first combat.
fn first_combat(seed: u64) -> sts2_core::run_manager::RunManager {
    install();
    let mut rm = new_run_manager_for_character(seed, CharacterId::Ironclad).with_neow_start();
    initialize_run_setup(&mut rm);
    for _ in 0..200 {
        let actions = rm.get_available_actions();
        assert!(!actions.is_empty(), "ran out of actions before combat");
        for &a in &actions {
            let mut probe = rm.clone();
            let _ = probe.take_action(a);
            if probe.phase() == Phase::Combat {
                let _ = rm.take_action(a);
                return rm;
            }
        }
        let _ = rm.take_action(actions[0]);
        assert_ne!(rm.phase(), Phase::RunOver);
    }
    panic!("no combat reached");
}

/// Win the (already entered) combat as a `room` fight with `setup` applied to
/// the run first. Returns the manager sitting on the reward screen.
fn win_in(
    base: &sts2_core::run_manager::RunManager,
    room: RoomType,
    act_index: u8,
    setup: impl FnOnce(&mut RunState),
) -> sts2_core::run_manager::RunManager {
    let mut rm = base.clone();
    rm.run_state.current_room = Some(Room::new(room));
    rm.run_state.act_index = act_index;
    rm.run_state.last_combat_gold_proportion = 1.0;
    setup(&mut rm.run_state);
    let r = rm.resolve_combat_fast(0.1, true);
    assert_ne!(r, ActionResult::InvalidForPhase);
    rm
}

fn extra_cards(rs: &mut RunState, room: RoomType) -> Vec<CardId> {
    rs.card_reward_gen_ctx = Some(CardRewardGenCtx {
        room_type: room,
        is_from_combat: false,
    });
    let cards = generate_card_reward_options(rs);
    rs.card_reward_gen_ctx = None;
    rs.card_reward_upgraded_flags.clear();
    rs.card_reward_enchantments.clear();
    cards
}

/// `new RelicReward(player)` populate + oracle auto-collect.
fn extra_relic(rs: &mut RunState) {
    rs.offer_random_relic_rewards(1, &[None]);
    rs.drain_relic_rewards_directly();
}

fn cursor(rs: &RunState) -> u32 {
    rs.rng.rewards.counter()
}

const SEEDS: [u64; 3] = [11, 29, 77];

// ---------------------------------------------------------------------------
// Per-relic stream consumption
// ---------------------------------------------------------------------------

/// `PrayerWheel.cs:15-26`: second CardReward `ForRoom(Monster)`, populated after
/// the room's own card reward. Not offered (the oracle presents only the set's
/// first CardReward); its draws are consumed.
#[test]
fn prayer_wheel_extra_card_reward_consumes_the_rewards_stream() {
    for seed in SEEDS {
        let base = first_combat(seed);
        let plain = win_in(&base, RoomType::Monster, 1, |_| {});
        let live = win_in(&base, RoomType::Monster, 1, |rs| {
            rs.relics.push(RelicId::PrayerWheel);
        });
        let mut want = plain.clone();
        let extra = extra_cards(&mut want.run_state, RoomType::Monster);
        assert_eq!(extra.len(), 3);
        assert!(cursor(&want.run_state) > cursor(&plain.run_state));
        assert_eq!(
            cursor(&live.run_state),
            cursor(&want.run_state),
            "seed {seed}"
        );
        assert_eq!(live.card_reward_options(), plain.card_reward_options());
        assert_eq!(live.run_state.unoffered_card_rewards.len(), 1);
        assert_eq!(live.run_state.unoffered_card_rewards[0].options, extra);
        assert_eq!(live.run_state.gold, plain.run_state.gold);
        assert_eq!(
            live.run_state.card_rarity_odds, want.run_state.card_rarity_odds,
            "pity state advances with the extra reward"
        );
        // Only on Monster rooms.
        let elite = win_in(&base, RoomType::Elite, 1, |rs| {
            rs.relics.push(RelicId::PrayerWheel);
        });
        assert!(elite.run_state.unoffered_card_rewards.is_empty());
    }
}

/// `WhiteStar.cs:21-32`: `ForRoom(Owner, RoomType.Boss)` (all-Rare odds) even in an
/// Elite room.
#[test]
fn white_star_extra_card_reward_uses_boss_context() {
    for seed in SEEDS {
        let base = first_combat(seed);
        let plain = win_in(&base, RoomType::Elite, 1, |_| {});
        let live = win_in(&base, RoomType::Elite, 1, |rs| {
            rs.relics.push(RelicId::WhiteStar);
        });
        let mut want = plain.clone();
        let extra = extra_cards(&mut want.run_state, RoomType::Boss);
        let mut elite_ctx = plain.clone();
        let wrong = extra_cards(&mut elite_ctx.run_state, RoomType::Elite);
        assert_eq!(
            cursor(&live.run_state),
            cursor(&want.run_state),
            "seed {seed}"
        );
        assert_eq!(live.run_state.unoffered_card_rewards[0].options, extra);
        assert_ne!(
            extra, wrong,
            "boss odds differ from elite odds (seed {seed})"
        );
        assert_eq!(live.card_reward_options(), plain.card_reward_options());
    }
}

/// `BlackStar.cs`: elite, `new RelicReward(player)` => one `NextFloat` rarity
/// roll (`RelicFactory.cs:54-63`) + front pull, collected after the elite's own
/// relic.
#[test]
fn black_star_adds_one_rarity_roll_and_a_second_elite_relic() {
    for seed in SEEDS {
        let base = first_combat(seed);
        let plain = win_in(&base, RoomType::Elite, 1, |_| {});
        let live = win_in(&base, RoomType::Elite, 1, |rs| {
            rs.relics.push(RelicId::BlackStar);
        });
        let mut want = plain.clone();
        extra_relic(&mut want.run_state);
        assert_eq!(
            cursor(&live.run_state),
            cursor(&plain.run_state) + 1,
            "seed {seed}"
        );
        assert_eq!(cursor(&live.run_state), cursor(&want.run_state));
        // [..., BlackStar itself, elite relic, extra relic]
        let mut want_relics = want.run_state.relics.clone();
        want_relics.insert(base.run_state.relics.len(), RelicId::BlackStar);
        assert_eq!(live.run_state.relics, want_relics, "seed {seed}");
        assert_eq!(
            live.run_state.relics.len(),
            plain.run_state.relics.len() + 2
        );
        assert!(live.run_state.relic_rewards.is_empty(), "auto-collected");
        // Monster rooms: nothing.
        let m_plain = win_in(&base, RoomType::Monster, 1, |_| {});
        let m_live = win_in(&base, RoomType::Monster, 1, |rs| {
            rs.relics.push(RelicId::BlackStar);
        });
        assert_eq!(cursor(&m_live.run_state), cursor(&m_plain.run_state));
    }
}

/// `WongosMysteryTicket.cs`: after 5 combats, 3 `RelicReward(player)` (3 rarity
/// rolls) on a combat room, once (`GaveRelic`).
#[test]
fn wongos_mystery_ticket_adds_three_relics_once() {
    for seed in SEEDS {
        let base = first_combat(seed);
        let plain = win_in(&base, RoomType::Monster, 1, |_| {});
        let live = win_in(&base, RoomType::Monster, 1, |rs| {
            rs.relics.push(RelicId::WongosMysteryTicket);
            rs.relic_counters.insert(RelicId::WongosMysteryTicket, 5);
        });
        let mut want = plain.clone();
        for _ in 0..3 {
            extra_relic(&mut want.run_state);
        }
        assert_eq!(
            cursor(&live.run_state),
            cursor(&plain.run_state) + 3,
            "seed {seed}"
        );
        let mut want_relics = want.run_state.relics.clone();
        want_relics.insert(base.run_state.relics.len(), RelicId::WongosMysteryTicket);
        assert_eq!(live.run_state.relics, want_relics, "seed {seed}");
        assert_eq!(
            live.run_state
                .relic_counters
                .get(&RelicId::WongosMysteryTicket),
            Some(&-1),
            "GaveRelic latch"
        );
        // `AfterCombatEnd` counts this fight first (`WongosMysteryTicket.cs`
        // `CombatsFinished++` before rewards): 4 -> 5 fires, 3 -> 4 does not.
        let fires_at_4 = win_in(&base, RoomType::Monster, 1, |rs| {
            rs.relics.push(RelicId::WongosMysteryTicket);
            rs.relic_counters.insert(RelicId::WongosMysteryTicket, 4);
        });
        assert_eq!(cursor(&fires_at_4.run_state), cursor(&plain.run_state) + 3);
        let early = win_in(&base, RoomType::Monster, 1, |rs| {
            rs.relics.push(RelicId::WongosMysteryTicket);
            rs.relic_counters.insert(RelicId::WongosMysteryTicket, 3);
        });
        assert_eq!(cursor(&early.run_state), cursor(&plain.run_state));
        // Latched: a second reward set adds nothing.
        let mut again = live.run_state.clone();
        let mut rewards = Vec::new();
        let _ = dispatch_modify_rewards(&mut again, &mut rewards, RoomType::Monster);
        assert!(rewards.is_empty());
    }
}

/// `LavaRock.cs`: Act 1 boss only, 2 `RelicReward(player)`, once.
#[test]
fn lava_rock_adds_two_relics_after_the_act_one_boss() {
    for seed in SEEDS {
        let base = first_combat(seed);
        let plain = win_in(&base, RoomType::Boss, 1, |_| {});
        let live = win_in(&base, RoomType::Boss, 1, |rs| {
            rs.relics.push(RelicId::LavaRock);
        });
        let mut want = plain.clone();
        for _ in 0..2 {
            extra_relic(&mut want.run_state);
        }
        assert_eq!(
            cursor(&live.run_state),
            cursor(&plain.run_state) + 2,
            "seed {seed}"
        );
        let mut want_relics = want.run_state.relics.clone();
        want_relics.insert(base.run_state.relics.len(), RelicId::LavaRock);
        assert_eq!(live.run_state.relics, want_relics, "seed {seed}");
        assert_eq!(
            live.run_state.relic_counters.get(&RelicId::LavaRock),
            Some(&1)
        );
        // Act 2 boss / non-boss: nothing.
        let act2 = win_in(&base, RoomType::Boss, 2, |rs| {
            rs.relics.push(RelicId::LavaRock);
        });
        let act2_plain = win_in(&base, RoomType::Boss, 2, |_| {});
        assert_eq!(cursor(&act2.run_state), cursor(&act2_plain.run_state));
        let monster = win_in(&base, RoomType::Monster, 1, |rs| {
            rs.relics.push(RelicId::LavaRock);
        });
        let monster_plain = win_in(&base, RoomType::Monster, 1, |_| {});
        assert_eq!(cursor(&monster.run_state), cursor(&monster_plain.run_state));
    }
}

/// `AmethystAubergine.cs`: +15 gold on Monster / Elite / non-final Boss; a fixed
/// `GoldReward(15)` never populates, so no stream draws.
#[test]
fn amethyst_aubergine_adds_gold_without_stream_draws() {
    for seed in SEEDS {
        let base = first_combat(seed);
        for (room, act) in [
            (RoomType::Monster, 1u8),
            (RoomType::Elite, 1),
            (RoomType::Boss, 1),
        ] {
            let plain = win_in(&base, room, act, |_| {});
            let live = win_in(&base, room, act, |rs| {
                rs.relics.push(RelicId::AmethystAubergine);
            });
            assert_eq!(
                live.run_state.gold,
                plain.run_state.gold + 15,
                "{room:?} seed {seed}"
            );
            assert_eq!(cursor(&live.run_state), cursor(&plain.run_state));
            assert_eq!(live.card_reward_options(), plain.card_reward_options());
        }
    }
}

// ---------------------------------------------------------------------------
// Listener order: acquisition order, normal pass then Late pass
// ---------------------------------------------------------------------------

/// WhiteStar (card, 3 interleaves) and BlackStar (1 float) both populate after the
/// elite's own rewards; acquisition order decides which draws first, so the cards
/// and the relic differ between the two orders.
#[test]
fn elite_white_star_and_black_star_follow_acquisition_order() {
    for seed in SEEDS {
        let base = first_combat(seed);
        let plain = win_in(&base, RoomType::Elite, 1, |_| {});
        let ws_first = win_in(&base, RoomType::Elite, 1, |rs| {
            rs.relics.extend([RelicId::WhiteStar, RelicId::BlackStar]);
        });
        let bs_first = win_in(&base, RoomType::Elite, 1, |rs| {
            rs.relics.extend([RelicId::BlackStar, RelicId::WhiteStar]);
        });

        let mut want_a = plain.clone();
        let cards_a = extra_cards(&mut want_a.run_state, RoomType::Boss);
        extra_relic(&mut want_a.run_state);
        let mut want_b = plain.clone();
        extra_relic(&mut want_b.run_state);
        let cards_b = extra_cards(&mut want_b.run_state, RoomType::Boss);

        assert_eq!(
            ws_first.run_state.unoffered_card_rewards[0].options, cards_a,
            "seed {seed}"
        );
        assert_eq!(
            bs_first.run_state.unoffered_card_rewards[0].options, cards_b,
            "seed {seed}"
        );
        let tail = |rm: &sts2_core::run_manager::RunManager| {
            rm.run_state.relics[plain.run_state.relics.len() + 2..].to_vec()
        };
        let tail_a: Vec<RelicId> = want_a.run_state.relics[plain.run_state.relics.len()..].to_vec();
        let tail_b: Vec<RelicId> = want_b.run_state.relics[plain.run_state.relics.len()..].to_vec();
        assert_eq!(tail(&ws_first), tail_a, "seed {seed}");
        assert_eq!(tail(&bs_first), tail_b, "seed {seed}");
        assert_eq!(cursor(&ws_first.run_state), cursor(&bs_first.run_state));
        assert_eq!(cursor(&ws_first.run_state), cursor(&want_a.run_state));
    }
    // The orders are distinguishable on at least one seed (otherwise the test
    // above would not prove an ordering).
    let distinct = SEEDS.iter().any(|&seed| {
        let base = first_combat(seed);
        let a = win_in(&base, RoomType::Elite, 1, |rs| {
            rs.relics.extend([RelicId::WhiteStar, RelicId::BlackStar]);
        });
        let b = win_in(&base, RoomType::Elite, 1, |rs| {
            rs.relics.extend([RelicId::BlackStar, RelicId::WhiteStar]);
        });
        a.run_state.unoffered_card_rewards[0].options
            != b.run_state.unoffered_card_rewards[0].options
    });
    assert!(
        distinct,
        "acquisition order must change the stream on some seed"
    );
}

/// PrayerWheel (card) + Wongos (3 relics) on a Monster room, both orders.
#[test]
fn monster_prayer_wheel_and_wongos_follow_acquisition_order() {
    for seed in SEEDS {
        let base = first_combat(seed);
        let plain = win_in(&base, RoomType::Monster, 1, |_| {});
        let setup = |first: RelicId, second: RelicId| {
            move |rs: &mut RunState| {
                rs.relics.extend([first, second]);
                rs.relic_counters.insert(RelicId::WongosMysteryTicket, 5);
            }
        };
        let wheel_first = win_in(
            &base,
            RoomType::Monster,
            1,
            setup(RelicId::PrayerWheel, RelicId::WongosMysteryTicket),
        );
        let wongos_first = win_in(
            &base,
            RoomType::Monster,
            1,
            setup(RelicId::WongosMysteryTicket, RelicId::PrayerWheel),
        );
        let mut want_a = plain.clone();
        let cards_a = extra_cards(&mut want_a.run_state, RoomType::Monster);
        for _ in 0..3 {
            extra_relic(&mut want_a.run_state);
        }
        let mut want_b = plain.clone();
        for _ in 0..3 {
            extra_relic(&mut want_b.run_state);
        }
        let cards_b = extra_cards(&mut want_b.run_state, RoomType::Monster);
        assert_eq!(
            wheel_first.run_state.unoffered_card_rewards[0].options, cards_a,
            "seed {seed}"
        );
        assert_eq!(
            wongos_first.run_state.unoffered_card_rewards[0].options, cards_b,
            "seed {seed}"
        );
        let n = plain.run_state.relics.len();
        assert_eq!(
            wheel_first.run_state.relics[n + 2..],
            want_a.run_state.relics[n..]
        );
        assert_eq!(
            wongos_first.run_state.relics[n + 2..],
            want_b.run_state.relics[n..]
        );
        assert_eq!(cursor(&wheel_first.run_state), cursor(&want_a.run_state));
        assert_eq!(cursor(&wongos_first.run_state), cursor(&want_b.run_state));
    }
}

/// Reward-list order (what the listeners append, in order) straight off the
/// dispatcher, for the same acquisition permutations: pure list assertions with
/// the C# listener bodies (owner/room gates) and no stream involved.
#[test]
fn dispatch_reward_list_order_follows_acquisition_order() {
    install();
    let list = |relics: &[RelicId], room: RoomType| {
        let mut rs = RunState::new();
        rs.relics.extend_from_slice(relics);
        rs.relic_counters.insert(RelicId::WongosMysteryTicket, 5);
        let mut rewards: Vec<Reward> = Vec::new();
        let _ = dispatch_modify_rewards(&mut rs, &mut rewards, room);
        rewards
    };
    let kinds = |rewards: &[Reward]| -> Vec<&'static str> {
        rewards
            .iter()
            .map(|r| match r {
                Reward::Card(_) => "card",
                Reward::Relic(_) => "relic",
                Reward::Gold { .. } => "gold",
                _ => "other",
            })
            .collect()
    };
    use RelicId::{AmethystAubergine as Am, BlackStar as Bs, PrayerWheel as Pw, WhiteStar as Ws};
    use RelicId::{Driftwood as Dw, WongosMysteryTicket as Wg};
    // Monster: PrayerWheel (card), Wongos (3 relics), Amethyst (gold).
    assert_eq!(
        kinds(&list(&[Pw, Wg, Am], RoomType::Monster)),
        ["card", "relic", "relic", "relic", "gold"]
    );
    assert_eq!(
        kinds(&list(&[Am, Wg, Pw], RoomType::Monster)),
        ["gold", "relic", "relic", "relic", "card"]
    );
    // Elite: WhiteStar (card), BlackStar (relic), Amethyst (gold), Wongos.
    assert_eq!(
        kinds(&list(&[Bs, Ws, Am], RoomType::Elite)),
        ["relic", "card", "gold"]
    );
    assert_eq!(
        kinds(&list(&[Am, Ws, Bs], RoomType::Elite)),
        ["gold", "card", "relic"]
    );
    // Driftwood's Late pass runs after ALL normal listeners regardless of when it
    // was acquired, and flags every CardReward in the set (including the
    // wheel's, which an earlier-acquired Driftwood has not "seen" yet).
    for order in [[Dw, Pw], [Pw, Dw]] {
        let rewards = list(&order, RoomType::Monster);
        assert_eq!(rewards.len(), 1, "{order:?}");
        let Reward::Card(c) = &rewards[0] else {
            panic!("expected a card reward");
        };
        assert!(c.can_reroll, "{order:?}");
        assert_eq!(c.room_context, Some(RoomType::Monster));
    }
}

// ---------------------------------------------------------------------------
// RL-B10 Driftwood / CardReward.Reroll
// ---------------------------------------------------------------------------

/// `Driftwood.TryModifyRewardsLate` only touches CardRewards, returns true even
/// with none, and only for the owner.
#[test]
fn driftwood_late_flags_every_card_reward() {
    install();
    let mut rs = RunState::new();
    let mut rewards = vec![
        Reward::Gold { amount: 3 },
        Reward::Card(CardReward::from_options(vec![CardId::Bash])),
        Reward::Card(CardReward::for_room(RoomType::Boss)),
    ];
    // Not owned: untouched.
    let _ = dispatch_modify_rewards(&mut rs, &mut rewards, RoomType::Elite);
    assert!(rewards
        .iter()
        .all(|r| !matches!(r, Reward::Card(c) if c.can_reroll)));
    rs.relics.push(RelicId::Driftwood);
    let result = dispatch_modify_rewards(&mut rs, &mut rewards, RoomType::Elite);
    assert_eq!(result, sts2_core::EffectResult::Resolved);
    let flagged: Vec<bool> = rewards
        .iter()
        .filter_map(|r| match r {
            Reward::Card(c) => Some(c.can_reroll),
            _ => None,
        })
        .collect();
    assert_eq!(flagged, [true, true]);
    // No card reward at all: still `true` (resolved).
    let mut only_gold = vec![Reward::Gold { amount: 1 }];
    assert_eq!(
        dispatch_modify_rewards(&mut rs, &mut only_gold, RoomType::Monster),
        sts2_core::EffectResult::Resolved
    );
}

/// The oracle's `LastingCandy.TryModifyCardRewardOptions` ignores rewards without
/// `IsFromCombat` (`LastingCandy.cs:120`); listener-appended CardRewards have none.
#[test]
fn listener_appended_card_rewards_are_not_from_combat() {
    install();
    let base = first_combat(5);
    let mut rs = base.run_state.clone();
    rs.relics.push(RelicId::LastingCandy);
    rs.relic_counters.insert(RelicId::LastingCandy, 2);
    let mut rs2 = rs.clone();
    let from_combat = generate_card_reward_options(&mut rs);
    let appended = extra_cards(&mut rs2, RoomType::Monster);
    assert_eq!(
        from_combat.len(),
        4,
        "candy adds a Power to a combat reward"
    );
    assert_eq!(appended.len(), 3, "but not to a listener-appended reward");
}

fn has_reroll(rm: &sts2_core::run_manager::RunManager) -> bool {
    rm.get_available_actions()
        .contains(&RunAction::RerollCardReward)
}

/// One reroll per reward: regenerates the 3 options off the same room context
/// (`RerollOptions`), consuming exactly another `CreateForReward` interleave, then
/// `CanReroll` is false.
#[test]
fn driftwood_reroll_once_regenerates_the_options() {
    for seed in SEEDS {
        let base = first_combat(seed);
        let plain = win_in(&base, RoomType::Monster, 1, |_| {});
        assert!(!has_reroll(&plain), "no Driftwood, no reroll");
        let mut live = win_in(&base, RoomType::Monster, 1, |rs| {
            rs.relics.push(RelicId::Driftwood);
        });
        assert_eq!(live.phase(), Phase::CardReward);
        assert!(has_reroll(&live), "seed {seed}");
        assert_eq!(live.card_reward_options(), plain.card_reward_options());
        assert_eq!(
            cursor(&live.run_state),
            cursor(&plain.run_state),
            "flagging draws nothing"
        );

        let mut want = plain.clone();
        want.run_state.card_reward_gen_ctx = None;
        let rerolled = generate_card_reward_options(&mut want.run_state);

        assert_eq!(
            live.take_action(RunAction::RerollCardReward),
            ActionResult::Advanced
        );
        assert_eq!(
            live.card_reward_options(),
            rerolled.as_slice(),
            "seed {seed}"
        );
        assert_eq!(cursor(&live.run_state), cursor(&want.run_state));
        assert_eq!(live.phase(), Phase::CardReward);
        assert!(!has_reroll(&live), "once per reward");
        assert_eq!(
            live.take_action(RunAction::RerollCardReward),
            ActionResult::InvalidForPhase
        );
        assert_eq!(
            cursor(&live.run_state),
            cursor(&want.run_state),
            "no second draw"
        );
        // The reroll leaves the pick/skip surface intact.
        assert!(live
            .get_available_actions()
            .contains(&RunAction::PickCardReward { idx: None }));
    }
}

/// Reroll is offered after Skip (C# `CardRewardAlternative.Generate` order: Skip,
/// Reroll, then hook alternatives).
#[test]
fn reroll_action_sits_between_skip_and_sacrifice() {
    let base = first_combat(SEEDS[0]);
    let rm = win_in(&base, RoomType::Monster, 1, |rs| {
        rs.relics.extend([RelicId::Driftwood, RelicId::PaelsWing]);
    });
    let acts = rm.get_available_actions();
    let skip = acts
        .iter()
        .position(|a| *a == RunAction::PickCardReward { idx: None })
        .unwrap();
    let reroll = acts
        .iter()
        .position(|a| *a == RunAction::RerollCardReward)
        .unwrap();
    let sac = acts
        .iter()
        .position(|a| *a == RunAction::SacrificeCardReward)
        .unwrap();
    assert!(skip < reroll && reroll < sac, "{acts:?}");
}

/// A pick resolves the reward: the flag does not leak into the next screen.
#[test]
fn reroll_flag_does_not_outlive_the_reward() {
    let base = first_combat(SEEDS[1]);
    let mut rm = win_in(&base, RoomType::Monster, 1, |rs| {
        rs.relics.push(RelicId::Driftwood);
    });
    assert!(has_reroll(&rm));
    let _ = rm.take_action(RunAction::PickCardReward { idx: Some(0) });
    assert!(!has_reroll(&rm));
    assert_eq!(
        rm.take_action(RunAction::RerollCardReward),
        ActionResult::InvalidForPhase
    );
}
