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
    assert_eq!(state.creatures[0].current_hp, 1, "2 plays x 3 hits x 3 = 18");
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
