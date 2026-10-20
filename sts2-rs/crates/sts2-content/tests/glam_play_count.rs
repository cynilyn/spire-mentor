//! B13 (0c repair): a Glam-enchanted card's FIRST play replays once
//! (C# `Glam.EnchantPlayCount`, `Glam.cs:34-40`), through the real
//! `combat_loop::play_card` + content registry. Reproduces the 0c P0A17
//! step-33 shape: Nibbit HP 19, Sword Boomerang (3 hits x 3 damage) with
//! Glam goes 19 -> 1 on the first use and only 9 damage on the second.

use smallvec::SmallVec;
use std::num::NonZeroU32;

use sts2_content::cards::dispatch as content_dispatch;
use sts2_core::cards::{Card, CardId, CardType, EnchantmentId, TargetType};
use sts2_core::{
    CombatPhase, CombatState, Creature, CreatureUid, IntentHistory, MonsterId, MonsterKind,
};

fn fresh(enemy_hp: i32) -> CombatState {
    content_dispatch::ensure_installed();
    let mut state = CombatState::default();
    let id = NonZeroU32::new(1).expect("nz");
    state.creatures.push(Creature {
        id,
        current_hp: enemy_hp,
        max_hp: enemy_hp,
        block: 0,
        powers: SmallVec::new(),
        species: Some(MonsterKind::Nibbit),
        intent: None,
        intent_history: IntentHistory::default(),
        monster_id: MonsterId("Nibbit"),
        ai: None,
    });
    state.enemy_uids.push(CreatureUid(id));
    state.player.current_hp = 80;
    state.player.max_hp = 80;
    state.player.energy = 99;
    state.player.max_energy = 99;
    let _ = state.init_player_creature();
    state.phase = CombatPhase::PlayerPlayPhase;
    state
}

fn push_boomerang(state: &mut CombatState, glam: bool) -> (sts2_core::CardUid, u8) {
    let raw = u32::try_from(state.cards.len() + 1).expect("u32");
    let uid = sts2_core::CardUid::new(raw).expect("nz");
    let mut card = Card {
        uid,
        card_id: CardId::SwordBoomerang,
        cost: 0,
        card_type: CardType::Attack,
        target_type: TargetType::Random,
        upgraded: false,
        base_damage: 3,
        base_block: 0,
        ..Card::pod_defaults()
    };
    if glam {
        card.set_enchantment(EnchantmentId::Glam, 1);
    }
    state.cards.push(card);
    state.player.hand.push(uid);
    (uid, u8::try_from(state.player.hand.len() - 1).expect("u8"))
}

fn play(state: &mut CombatState, idx: u8) {
    let _ = sts2_core::combat_loop::play_card(state, idx, None);
}

#[test]
fn glam_sword_boomerang_first_use_is_two_plays_of_three_hits() {
    // P0A17 step 33: Nibbit HP 19 -> 1.
    let mut state = fresh(19);
    let (_, idx) = push_boomerang(&mut state, true);
    play(&mut state, idx);
    assert_eq!(
        state.creatures[0].current_hp, 1,
        "2 plays x 3 hits x 3 = 18"
    );
}

#[test]
fn glam_sword_boomerang_second_use_is_one_play() {
    let mut state = fresh(60);
    let (uid, idx) = push_boomerang(&mut state, true);
    play(&mut state, idx);
    assert_eq!(state.creatures[0].current_hp, 60 - 18);
    // Replay the same card instance: the Glam latch is set, so 1 play only.
    state.player.discard.retain(|u| *u != uid);
    state.player.exhaust.retain(|u| *u != uid);
    state.player.hand.push(uid);
    let idx = u8::try_from(state.player.hand.len() - 1).expect("u8");
    play(&mut state, idx);
    assert_eq!(state.creatures[0].current_hp, 60 - 18 - 9, "1 play x 3 x 3");
}

#[test]
fn sword_boomerang_without_glam_is_one_play() {
    let mut state = fresh(19);
    let (_, idx) = push_boomerang(&mut state, false);
    play(&mut state, idx);
    assert_eq!(state.creatures[0].current_hp, 10, "3 hits x 3 = 9");
}

/// P0A5 step 35 shape (0.111.0 baseline): a Glam(1) Anger (6 damage) against a
/// Vulnerable enemy deals 6 x 1.5 = 9 per hit; the first use replays once, so
/// the enemy goes 48 -> 30 (two hits), not 48 -> 39 (one hit).
#[test]
fn glam_anger_vs_vulnerable_enemy_is_two_hits_of_nine() {
    use sts2_core::{Power, PowerKind, PowerUid};
    let mut state = fresh(48);
    let pid = PowerUid::new(1).expect("non-zero power uid");
    state.powers.push(Power {
        id: pid.0,
        kind: PowerKind::Vulnerable,
        amount: 2,
        counter: 0,
        applier: None,
        skip_next_tick: false,
    });
    state.creatures[0].powers.push(pid);
    let raw = u32::try_from(state.cards.len() + 1).expect("u32");
    let uid = sts2_core::CardUid::new(raw).expect("nz");
    let mut anger = Card {
        uid,
        card_id: CardId::Anger,
        cost: 0,
        card_type: CardType::Attack,
        target_type: TargetType::Enemy,
        upgraded: false,
        base_damage: 6,
        base_block: 0,
        ..Card::pod_defaults()
    };
    anger.set_enchantment(EnchantmentId::Glam, 1);
    state.cards.push(anger);
    state.player.hand.push(uid);
    let idx = u8::try_from(state.player.hand.len() - 1).expect("u8");
    let _ = sts2_core::combat_loop::play_card(&mut state, idx, Some(0));
    assert_eq!(
        state.creatures[0].current_hp,
        48 - 18,
        "2 plays x 6 x 1.5 = 18"
    );
}

// ── Same-UID regressions across auto-play and manual play ────────────────
// C# runs one `OnPlayWrapper` for both (`CardModel.cs:1895,1959`), and
// `Glam.AfterCardPlayed` (`Glam.cs:42-51`) latches after the first play of the
// card however it was played. Sword Boomerang draws `combat_targets` once per
// hit, so the draw count shows how many plays ran (first use 2 x 3 = 6 draws,
// later uses 3).

fn draws(state: &CombatState) -> u32 {
    state.run_rng.combat_targets.counter()
}

/// Put the (already played) card back in the discard pile, ready to auto-play.
fn to_discard(state: &mut CombatState, uid: sts2_core::CardUid) {
    state.player.hand.retain(|u| *u != uid);
    state.player.play.retain(|u| *u != uid);
    state.player.draw.retain(|u| *u != uid);
    state.player.exhaust.retain(|u| *u != uid);
    if !state.player.discard.contains(&uid) {
        state.player.discard.push(uid);
    }
}

fn to_hand(state: &mut CombatState, uid: sts2_core::CardUid) -> u8 {
    state.player.discard.retain(|u| *u != uid);
    state.player.play.retain(|u| *u != uid);
    state.player.exhaust.retain(|u| *u != uid);
    state.player.hand.retain(|u| *u != uid);
    state.player.hand.push(uid);
    u8::try_from(state.player.hand.len() - 1).expect("u8")
}

fn auto(state: &mut CombatState, uid: sts2_core::CardUid) {
    to_discard(state, uid);
    assert!(sts2_core::combat_loop::auto_play_specific_card_from_discard(state, uid, false));
}

/// Plays done = enemy HP lost / 9 (3 hits x 3 damage per play).
fn step(state: &mut CombatState, f: impl FnOnce(&mut CombatState)) -> (i32, u32) {
    let (hp0, d0) = (state.creatures[0].current_hp, draws(state));
    f(state);
    (hp0 - state.creatures[0].current_hp, draws(state) - d0)
}

#[test]
fn glam_auto_play_then_auto_play_same_uid_replays_only_the_first() {
    let mut state = fresh(400);
    let (uid, _) = push_boomerang(&mut state, true);
    assert_eq!(
        step(&mut state, |s| auto(s, uid)),
        (18, 6),
        "first auto: 2 plays"
    );
    assert_eq!(
        step(&mut state, |s| auto(s, uid)),
        (9, 3),
        "second auto: 1 play"
    );
    assert_eq!(
        step(&mut state, |s| auto(s, uid)),
        (9, 3),
        "third auto: 1 play"
    );
}

#[test]
fn glam_auto_play_then_manual_same_uid_replays_only_the_first() {
    let mut state = fresh(400);
    let (uid, _) = push_boomerang(&mut state, true);
    assert_eq!(
        step(&mut state, |s| auto(s, uid)),
        (18, 6),
        "first auto: 2 plays"
    );
    let idx = to_hand(&mut state, uid);
    assert_eq!(
        step(&mut state, |s| play(s, idx)),
        (9, 3),
        "manual after auto: Glam already consumed"
    );
}

#[test]
fn glam_manual_then_auto_same_uid_replays_only_the_first() {
    let mut state = fresh(400);
    let (uid, idx) = push_boomerang(&mut state, true);
    assert_eq!(
        step(&mut state, |s| play(s, idx)),
        (18, 6),
        "first manual: 2 plays"
    );
    assert_eq!(
        step(&mut state, |s| auto(s, uid)),
        (9, 3),
        "auto after manual: 1 play"
    );
}
