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
        colorless: false,
        uniform_rarity: None,
    });
    let cards = generate_card_reward_options(rs);
    rs.card_reward_gen_ctx = None;
    rs.card_reward_upgraded_flags.clear();
    rs.card_reward_enchantments.clear();
    cards
}

/// `new RelicReward(player)` populate followed IMMEDIATELY by its claim.
/// Production (and C#, `RewardsSet.cs:93-116`) claims only after every reward is
/// populated, so this helper is a valid reference only where an obtain has no side
/// effect on the `Rewards` stream (none of the relics in these tests); the ordering
/// itself is pinned by `rewards_are_populated_before_any_is_claimed`.
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

// ---------------------------------------------------------------------------
// Population-before-claim ordering (production defers the claim)
// ---------------------------------------------------------------------------

/// Listener under an owned marker relic: every relic obtain draws one `Rewards`
/// float (an on-obtain side effect on the stream the populate loop uses).
fn obtain_draws_a_float(rs: &mut RunState) -> sts2_core::EffectResult {
    let _ = rs.rng.rewards.next_float(1.0);
    sts2_core::EffectResult::Resolved
}

/// `RewardsSet.cs:93-116` populates EVERY reward before any is collected. With an
/// on-obtain `Rewards` draw, `[Wongos, PrayerWheel]` must populate 3 relics, then
/// the card, and only then claim (3 side-effect draws). Claiming each relic right
/// after its populate yields a different card slate.
#[test]
fn rewards_are_populated_before_any_is_claimed() {
    use sts2_core::run_effects::{
        lookup_after_obtained, register_after_obtained, AfterObtainedRunFn,
    };
    let marker = RelicId::BagOfMarbles;
    assert!(
        lookup_after_obtained(marker).is_none(),
        "marker id must be free"
    );
    register_after_obtained(marker, obtain_draws_a_float as AfterObtainedRunFn);
    let mut distinct = false;
    for seed in SEEDS {
        let base = first_combat(seed);
        let setup = |rs: &mut RunState| {
            rs.relics
                .extend([marker, RelicId::WongosMysteryTicket, RelicId::PrayerWheel]);
            rs.relic_counters.insert(RelicId::WongosMysteryTicket, 5);
        };
        let live = win_in(&base, RoomType::Monster, 1, setup);
        let plain = win_in(&base, RoomType::Monster, 1, |rs| {
            rs.relics.push(marker);
        });

        // Deferred (C#): populate 3 relics, populate the card, then claim all.
        let mut deferred = plain.clone();
        for _ in 0..3 {
            deferred.run_state.offer_random_relic_rewards(1, &[None]);
        }
        let cards_deferred = extra_cards(&mut deferred.run_state, RoomType::Monster);
        deferred.run_state.drain_relic_rewards_directly();
        // Claim-as-you-populate.
        let mut eager = plain.clone();
        for _ in 0..3 {
            extra_relic(&mut eager.run_state);
        }
        let cards_eager = extra_cards(&mut eager.run_state, RoomType::Monster);

        assert_eq!(
            live.run_state.unoffered_card_rewards[0].options, cards_deferred,
            "seed {seed}"
        );
        assert_eq!(
            cursor(&live.run_state),
            cursor(&deferred.run_state),
            "seed {seed}"
        );
        let n = plain.run_state.relics.len();
        assert_eq!(
            live.run_state.relics[n + 2..],
            deferred.run_state.relics[n..]
        );
        distinct |= cards_deferred != cards_eager;
    }
    assert!(
        distinct,
        "the fixture must distinguish the two orders on some seed"
    );
}

// ---------------------------------------------------------------------------
// Event / relic reward sets (`RewardsCmd.OfferCustom`) go through ModifyRewards
// ---------------------------------------------------------------------------

/// Brain Leech Rip (`BrainLeech.cs:51-58`): `OfferCustom([CardReward(colorless,
/// ForNonCombatWithDefaultOdds)])` runs `ModifyRewards`/Late, so Driftwood flags
/// it; the reroll reuses the reward's OWN colorless options (base odds, no upgrade
/// roll), once.
#[test]
fn driftwood_rerolls_brain_leech_rip_with_its_own_colorless_options() {
    use sts2_content::cards::spec_index::roll_colorless_reward_options_no_upgrade;
    use sts2_core::events::EventId;
    install();
    for seed in SEEDS {
        let start = |driftwood: bool| {
            let mut rm =
                new_run_manager_for_character(seed, CharacterId::Ironclad).with_neow_start();
            initialize_run_setup(&mut rm);
            if driftwood {
                rm.run_state.relics.push(RelicId::Driftwood);
            }
            rm.run_state.enter_event_room(EventId::BrainLeech);
            let _ = rm.take_action(RunAction::EventChoice { idx: 1 });
            rm
        };
        let plain = start(false);
        assert_eq!(plain.phase(), Phase::CardReward);
        assert!(!has_reroll(&plain), "no Driftwood, no reroll");
        let mut live = start(true);
        assert_eq!(live.phase(), Phase::CardReward);
        assert_eq!(live.card_reward_options(), plain.card_reward_options());
        assert!(has_reroll(&live), "seed {seed}");

        let mut want = live.clone();
        let rerolled = roll_colorless_reward_options_no_upgrade(&mut want.run_state, 3);
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
        assert_eq!(
            live.run_state.card_rarity_odds, want.run_state.card_rarity_odds,
            "no pity"
        );
        assert!(!has_reroll(&live), "once");
        assert_eq!(
            live.take_action(RunAction::RerollCardReward),
            ActionResult::InvalidForPhase
        );
    }
}

/// Relic-offered sets (Lost Coffer: `OfferCustom([CardReward, PotionReward])`) are
/// routed the same way; their reroll uses the base-odds character-pool roller.
#[test]
fn driftwood_rerolls_lost_coffer_card_reward_at_base_odds() {
    use sts2_core::NeowRelic;
    install();
    let start = |driftwood: bool| {
        let mut rm = new_run_manager_for_character(3, CharacterId::Ironclad).with_neow_start();
        initialize_run_setup(&mut rm);
        rm.run_state.direct_event_relic_grant = true;
        let mut offers = rm.run_state.neow_offers.expect("neow offers rolled");
        offers.positive_2 = NeowRelic::LostCoffer;
        rm.run_state.neow_offers = Some(offers);
        if driftwood {
            rm.run_state.relics.push(RelicId::Driftwood);
        }
        let _ = rm.take_action(RunAction::EventChoice { idx: 1 });
        rm
    };
    let plain = start(false);
    assert_eq!(plain.phase(), Phase::CardReward);
    assert!(!has_reroll(&plain));
    let mut live = start(true);
    assert!(has_reroll(&live));
    let mut want = live.clone();
    want.run_state.card_reward_gen_ctx = None;
    let rerolled =
        sts2_content::cards::spec_index::roll_noncombat_card_reward_options(&mut want.run_state);
    assert_eq!(
        live.take_action(RunAction::RerollCardReward),
        ActionResult::Advanced
    );
    assert_eq!(live.card_reward_options(), rerolled.as_slice());
    assert_eq!(cursor(&live.run_state), cursor(&want.run_state));
    assert!(!has_reroll(&live));
}

/// NOT routed: Brain Leech Share Knowledge is a `CardSelectCmd` grid pick, not a
/// `RewardsSet`, so Driftwood never makes it rerollable. Pre-resolved slates whose
/// reroll options the engine cannot reproduce (Kaleidoscope) are never offered a
/// reroll either.
#[test]
fn card_select_screens_and_unsupported_slates_are_not_rerollable() {
    use sts2_core::events::EventId;
    install();
    let mut rm = new_run_manager_for_character(SEEDS[0], CharacterId::Ironclad).with_neow_start();
    initialize_run_setup(&mut rm);
    rm.run_state.relics.push(RelicId::Driftwood);
    rm.run_state.enter_event_room(EventId::BrainLeech);
    let _ = rm.take_action(RunAction::EventChoice { idx: 0 });
    assert!(
        !has_reroll(&rm),
        "ShareKnowledge is a card select, not a reward"
    );
    assert_eq!(
        CardReward::from_options(vec![CardId::Bash]).reroll,
        sts2_core::runstate::RerollSpec::Unsupported
    );
}

// ---------------------------------------------------------------------------
// Custom sets: no room context; unsupported rerolls are not advertised; GlassEye
// ---------------------------------------------------------------------------

/// Neow run on the event screen with a Lost Coffer offer; `pre` runs on the run
/// state first (queueing rewards ahead of the Lost Coffer card + potion), and
/// `current_room` is forced to `room`. Takes the Lost Coffer option so the queue
/// drains.
fn coffer_with(
    seed: u64,
    room: Option<RoomType>,
    relics: &[RelicId],
    pre: impl FnOnce(&mut RunState),
) -> sts2_core::run_manager::RunManager {
    use sts2_core::NeowRelic;
    install();
    let mut rm = new_run_manager_for_character(seed, CharacterId::Ironclad).with_neow_start();
    initialize_run_setup(&mut rm);
    rm.run_state.direct_event_relic_grant = true;
    let mut offers = rm.run_state.neow_offers.expect("neow offers rolled");
    offers.positive_2 = NeowRelic::LostCoffer;
    rm.run_state.neow_offers = Some(offers);
    rm.run_state.relics.extend_from_slice(relics);
    if let Some(room) = room {
        rm.run_state.current_room = Some(Room::new(room));
    }
    pre(&mut rm.run_state);
    let _ = rm.take_action(RunAction::EventChoice { idx: 1 });
    rm
}

/// C# `RewardsSet.WithCustomRewards` leaves `Room == null`: a queued custom set
/// drained while standing in a Monster room (e.g. a DreamCatcher reward after the
/// next fight) must not re-trigger the room-gated listeners.
#[test]
fn custom_reward_sets_have_no_room_context() {
    for seed in SEEDS {
        let owned = [
            RelicId::PrayerWheel,
            RelicId::AmethystAubergine,
            RelicId::Driftwood,
        ];
        let plain = coffer_with(seed, Some(RoomType::Monster), &[RelicId::Driftwood], |_| {});
        let live = coffer_with(seed, Some(RoomType::Monster), &owned, |_| {});
        assert!(
            live.run_state.unoffered_card_rewards.is_empty(),
            "seed {seed}"
        );
        assert_eq!(
            live.run_state.gold, plain.run_state.gold,
            "no Amethyst gold"
        );
        assert_eq!(cursor(&live.run_state), cursor(&plain.run_state));
        assert_eq!(live.card_reward_options(), plain.card_reward_options());
        // The Late pass still ran with a null room: Driftwood flagged the reward.
        assert!(has_reroll(&live));
    }
}

/// A slate the engine cannot reroll (Kaleidoscope-style, `RerollSpec::Unsupported`)
/// is not advertised even though Driftwood set its `CanReroll`; the next supported
/// reward in the same set still is. Asserted on the real legal-action list.
#[test]
fn unsupported_slates_do_not_advertise_a_reroll() {
    let slate = vec![CardId::Bash, CardId::Anger, CardId::Headbutt];
    let queued = slate.clone();
    let mut rm = coffer_with(SEEDS[0], None, &[RelicId::Driftwood], move |rs| {
        rs.pending_rewards
            .push(Reward::Card(CardReward::from_options(queued)));
    });
    assert_eq!(rm.phase(), Phase::CardReward);
    assert_eq!(rm.card_reward_options(), slate.as_slice());
    assert!(!has_reroll(&rm), "{:?}", rm.get_available_actions());
    assert_eq!(
        rm.take_action(RunAction::RerollCardReward),
        ActionResult::InvalidForPhase
    );
    // Resolve it: the following Lost Coffer card reward (supported) is rerollable.
    let _ = rm.take_action(RunAction::PickCardReward { idx: None });
    assert_eq!(rm.phase(), Phase::CardReward);
    assert!(has_reroll(&rm));
}

/// GlassEye (`GlassEye.cs:14-34`): five rewards of fixed rarity C, C, U, U, R at
/// uniform odds (three distinct picks of that rarity, no rarity roll, no upgrade
/// roll); a Driftwood reroll keeps the rarity and the uniform options.
#[test]
fn glass_eye_rewards_keep_their_fixed_rarity_through_a_reroll() {
    use sts2_content::cards::spec_index::{rarity_for_card_id, sample_distinct_cards_of_rarity};
    use sts2_content::cards::CardRarity;
    let expected = [
        CardRarity::Common,
        CardRarity::Common,
        CardRarity::Uncommon,
        CardRarity::Uncommon,
        CardRarity::Rare,
    ];
    for seed in SEEDS {
        let mut rm = coffer_with(seed, None, &[RelicId::Driftwood, RelicId::GlassEye], |rs| {
            rs.last_obtained_relic = Some(RelicId::GlassEye);
            let _ = sts2_core::run_effects::dispatch_after_obtained(rs);
            assert_eq!(rs.pending_rewards.len(), 5, "five queued rewards");
        });
        for (i, rarity) in expected.iter().enumerate() {
            assert_eq!(rm.phase(), Phase::CardReward, "reward {i}");
            let offered = rm.card_reward_options().to_vec();
            assert_eq!(offered.len(), 3, "reward {i}");
            assert!(
                offered
                    .iter()
                    .all(|id| rarity_for_card_id(*id) == Some(*rarity)),
                "reward {i} fixed rarity {rarity:?}"
            );
            assert!(has_reroll(&rm), "reward {i}");
            let mut want = rm.clone();
            let rerolled = sample_distinct_cards_of_rarity(
                &mut want.run_state,
                CharacterId::Ironclad,
                *rarity,
                3,
            );
            assert_eq!(
                rm.take_action(RunAction::RerollCardReward),
                ActionResult::Advanced
            );
            assert_eq!(rm.card_reward_options(), rerolled.as_slice(), "reward {i}");
            assert!(
                rerolled
                    .iter()
                    .all(|id| rarity_for_card_id(*id) == Some(*rarity)),
                "rerolled reward {i} keeps rarity"
            );
            assert_eq!(cursor(&rm.run_state), cursor(&want.run_state));
            assert!(!has_reroll(&rm));
            let _ = rm.take_action(RunAction::PickCardReward { idx: None });
        }
    }
}

/// GlassEye's rewards go through `CardFactory.CreateForReward`'s modification
/// hooks (`CardFactory.cs:84-87`; options carry `IsCardReward`, no `NoModifyHooks`).
/// With SilverCrucible (`SilverCrucible.cs:94-123`: `Cards` var = 3; every reward
/// while `TimesUsed < 3` is upgraded, then `TimesUsed++`) owned, the five rewards —
/// all populated up front by `OfferCustom` — come out as: rewards 1-3 fully
/// upgraded, rewards 4-5 plain, `TimesUsed == 3`. A later reroll finds the budget
/// spent and is plain. Expectations are the C# numbers above, not a re-roll.
#[test]
fn glass_eye_rewards_run_the_silver_crucible_hook() {
    for seed in SEEDS {
        let mut rm = coffer_with(
            seed,
            None,
            &[
                RelicId::Driftwood,
                RelicId::GlassEye,
                RelicId::SilverCrucible,
            ],
            |rs| {
                rs.last_obtained_relic = Some(RelicId::GlassEye);
                let _ = sts2_core::run_effects::dispatch_after_obtained(rs);
                assert_eq!(rs.pending_rewards.len(), 5);
            },
        );
        // SilverCrucible `TimesUsed` is the low byte of its packed counter.
        let used = rm.run_state.relic_counters[&RelicId::SilverCrucible] & 0xFF;
        assert_eq!(used, 3, "seed {seed}: 3 of the 5 populates consumed a use");
        for i in 0..5 {
            assert_eq!(rm.phase(), Phase::CardReward, "reward {i}");
            let flags = rm.card_reward_upgraded_flags().to_vec();
            assert_eq!(flags.len(), 3, "reward {i}");
            let want = i < 3;
            assert!(
                flags.iter().all(|f| *f == want),
                "reward {i}: upgraded == {want}, got {flags:?}"
            );
            if i == 0 {
                // The reroll re-fires the hooks with the budget already spent.
                assert_eq!(
                    rm.take_action(RunAction::RerollCardReward),
                    ActionResult::Advanced
                );
                let rerolled = rm.card_reward_upgraded_flags().to_vec();
                assert!(rerolled.iter().all(|f| !*f), "reroll plain: {rerolled:?}");
                assert_eq!(
                    rm.run_state.relic_counters[&RelicId::SilverCrucible] & 0xFF,
                    3
                );
                // Un-reroll check: pick from reward 2 later keeps its upgrade.
                let _ = rm.take_action(RunAction::PickCardReward { idx: None });
            } else if i == 1 {
                let before = rm.run_state.master_deck.len();
                let _ = rm.take_action(RunAction::PickCardReward { idx: Some(0) });
                assert_eq!(rm.run_state.master_deck.len(), before + 1);
                assert!(
                    rm.run_state.master_deck.last().expect("picked").upgraded,
                    "picked card keeps its offer-time upgrade"
                );
            } else {
                let _ = rm.take_action(RunAction::PickCardReward { idx: None });
            }
        }
    }
    // Control: without SilverCrucible nothing is upgraded.
    let rm = coffer_with(SEEDS[0], None, &[RelicId::GlassEye], |rs| {
        rs.last_obtained_relic = Some(RelicId::GlassEye);
        let _ = sts2_core::run_effects::dispatch_after_obtained(rs);
    });
    assert!(rm.card_reward_upgraded_flags().iter().all(|f| !*f));
}

// ---------------------------------------------------------------------------
// Non-combat OfferCustom rewards run the modification hooks too
// ---------------------------------------------------------------------------

/// `LostCoffer.cs:18-24`: `CardCreationOptions(pool, Other, RegularEncounter)`, no
/// `NoModifyHooks` => `CardFactory.CreateForReward` fires the hooks
/// (`CardFactory.cs:84-87`); `SilverCrucible.cs:94-123` (Cards var 3) upgrades every
/// card of the first 3 card rewards and counts a use per reward. So the Lost
/// Coffer reward arrives fully upgraded, `TimesUsed == 1`; a Driftwood reroll
/// re-populates it (hooks again, use 2, still upgraded); the picked card enters
/// the deck upgraded.
#[test]
fn lost_coffer_reward_and_reroll_run_the_silver_crucible_hook() {
    for seed in SEEDS {
        let mut rm = coffer_with(
            seed,
            None,
            &[RelicId::Driftwood, RelicId::SilverCrucible],
            |_| {},
        );
        assert_eq!(rm.phase(), Phase::CardReward);
        let used = |rm: &sts2_core::run_manager::RunManager| {
            rm.run_state.relic_counters[&RelicId::SilverCrucible] & 0xFF
        };
        assert_eq!(used(&rm), 1, "seed {seed}");
        assert_eq!(rm.card_reward_upgraded_flags(), [true, true, true]);
        assert_eq!(
            rm.take_action(RunAction::RerollCardReward),
            ActionResult::Advanced
        );
        assert_eq!(used(&rm), 2, "the reroll re-fires the hooks");
        assert_eq!(rm.card_reward_upgraded_flags(), [true, true, true]);
        let before = rm.run_state.master_deck.len();
        let _ = rm.take_action(RunAction::PickCardReward { idx: Some(0) });
        assert_eq!(rm.run_state.master_deck.len(), before + 1);
        assert!(rm.run_state.master_deck.last().expect("picked").upgraded);
    }
}

/// `Orrery.cs:14-32`: five card rewards `OfferCustom`ed together => all populated up
/// front; with SilverCrucible rewards 1-3 are fully upgraded, 4-5 plain,
/// `TimesUsed == 3`.
#[test]
fn orrery_rewards_run_the_silver_crucible_hook() {
    for seed in SEEDS {
        let mut rm = coffer_with(
            seed,
            None,
            &[RelicId::Orrery, RelicId::SilverCrucible],
            |rs| {
                rs.last_obtained_relic = Some(RelicId::Orrery);
                let _ = sts2_core::run_effects::dispatch_after_obtained(rs);
                assert!(rs.pending_rewards.len() >= 5, "Orrery queues its 5 rewards");
            },
        );
        let mut seen = Vec::new();
        for _ in 0..5 {
            assert_eq!(rm.phase(), Phase::CardReward);
            seen.push(rm.card_reward_upgraded_flags().to_vec());
            let _ = rm.take_action(RunAction::PickCardReward { idx: None });
        }
        for (i, flags) in seen.iter().enumerate() {
            assert_eq!(flags.len(), 3, "reward {i}");
            assert!(
                flags.iter().all(|f| *f == (i < 3)),
                "reward {i}: upgraded == {} got {flags:?}",
                i < 3
            );
        }
        assert_eq!(
            rm.run_state.relic_counters[&RelicId::SilverCrucible] & 0xFF,
            3
        );
    }
}

/// BrainLeech Rip (`BrainLeech.cs:51-57`, no `NoModifyHooks`): the colorless
/// reward is populated through the hooks (SilverCrucible upgrades all 3, one use)
/// and its reroll re-fires them.
#[test]
fn brain_leech_rip_reward_runs_the_silver_crucible_hook() {
    use sts2_core::events::EventId;
    install();
    let mut rm = new_run_manager_for_character(SEEDS[1], CharacterId::Ironclad).with_neow_start();
    initialize_run_setup(&mut rm);
    rm.run_state
        .relics
        .extend([RelicId::Driftwood, RelicId::SilverCrucible]);
    rm.run_state.enter_event_room(EventId::BrainLeech);
    let _ = rm.take_action(RunAction::EventChoice { idx: 1 });
    assert_eq!(rm.phase(), Phase::CardReward);
    assert_eq!(rm.card_reward_upgraded_flags(), [true, true, true]);
    assert_eq!(
        rm.run_state.relic_counters[&RelicId::SilverCrucible] & 0xFF,
        1
    );
    let _ = rm.take_action(RunAction::RerollCardReward);
    assert_eq!(rm.card_reward_upgraded_flags(), [true, true, true]);
    assert_eq!(
        rm.run_state.relic_counters[&RelicId::SilverCrucible] & 0xFF,
        2
    );
}

/// DreamCatcher (`DreamCatcher.cs:19`, offered via `HealRestSiteOption`'s
/// `OfferCustom`): `ForRoom(player, Monster)` — the Encounter-source roller with
/// pity — without `IsFromCombat`; the earlier sentinel used the base-odds roller.
#[test]
fn dream_catcher_reward_rolls_with_the_monster_room_context() {
    for seed in SEEDS {
        let mut want = None;
        let rm = coffer_with(seed, None, &[RelicId::Driftwood], |rs| {
            rs.pending_rewards
                .push(Reward::Card(CardReward::for_room(RoomType::Monster)));
            want = Some(rs.clone());
        });
        // The sentinel surfaces first; reference rolls from the state just before.
        let mut reference = want.expect("snapshot");
        let expect = extra_cards(&mut reference, RoomType::Monster);
        assert_eq!(rm.card_reward_options(), expect.as_slice(), "seed {seed}");
        assert!(has_reroll(&rm), "Driftwood flags it; Room spec rerolls");
    }
}
