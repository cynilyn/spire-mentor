//! R1c: combat divergences found by the 3-way triage (sts2-cli == oracle DLL, upstream Rust differs).
//!
//! Every fix is gated `>= V0_111_0`; each witness has a `_v0107` twin that pins the unchanged old behaviour.
//! Expectations come from the decompiled C# (`sts2-wiki/decompiled/v0.111.0/`), cited per test.

use smallvec::SmallVec;
use std::num::NonZeroU32;

use sts2_content::cards::dispatch as content_dispatch;
use sts2_content::cards::v0111::card_pod_for as card_pod_from_id_for;
use sts2_core::cards::CardId;
use sts2_core::potions::CharacterId;
use sts2_core::{
    CombatPhase, CombatState, Creature, CreatureUid, GameVersion, IntentHistory, MonsterId,
    MonsterKind, Power, PowerKind, PowerUid,
};

const V107: GameVersion = GameVersion::V0_107_1;
const V111: GameVersion = GameVersion::V0_111_0;

fn add_enemy(state: &mut CombatState, raw: u32, hp: i32) -> CreatureUid {
    let id = NonZeroU32::new(raw).expect("nz");
    state.creatures.push(Creature {
        id,
        current_hp: hp,
        max_hp: hp,
        block: 0,
        powers: SmallVec::new(),
        species: Some(MonsterKind::Nibbit),
        intent: None,
        intent_history: IntentHistory::default(),
        monster_id: MonsterId("Nibbit"),
        ai: None,
    });
    state.enemy_uids.push(CreatureUid(id));
    CreatureUid(id)
}

/// Player with 5 energy in the play phase, no enemies yet.
fn fresh(version: GameVersion, character: CharacterId) -> CombatState {
    content_dispatch::ensure_installed();
    sts2_content::relics::dispatch::ensure_installed();
    let mut state = CombatState {
        game_version: version,
        ..CombatState::default()
    };
    state.player.current_hp = 80;
    state.player.max_hp = 80;
    state.player.energy = 5;
    state.player.max_energy = 3;
    state.player.character = Some(character);
    let _ = state.init_player_creature();
    state.phase = CombatPhase::PlayerPlayPhase;
    state
}

fn new_card(state: &mut CombatState, id: CardId, upgraded: bool) -> sts2_core::CardUid {
    let raw = u32::try_from(state.cards.len() + 1).expect("u32");
    let uid = sts2_core::CardUid::new(raw).expect("nz");
    let mut card = card_pod_from_id_for(id, upgraded, state.game_version).expect("spec");
    card.uid = uid;
    state.cards.push(card);
    uid
}

fn push_hand(state: &mut CombatState, id: CardId, upgraded: bool) -> u8 {
    let uid = new_card(state, id, upgraded);
    state.player.hand.push(uid);
    u8::try_from(state.player.hand.len() - 1).expect("u8")
}

fn hp_of(state: &CombatState, uid: CreatureUid) -> i32 {
    state
        .creatures
        .iter()
        .find(|c| c.id == uid.0)
        .map_or(0, |c| c.current_hp)
}

fn add_player_power_with_applier(
    state: &mut CombatState,
    raw: u32,
    kind: PowerKind,
    amount: i32,
    applier: CreatureUid,
) {
    let pid = NonZeroU32::new(raw).expect("nz");
    let mut p = Power::new(pid, kind, amount);
    p.applier = Some(applier);
    state.powers.push(p);
    state.player.powers.push(PowerUid(pid));
}

// ── Thunderclap + Shrink whose applier dies in the same attack (seeds LSA_23, 24) ──

/// Enemy B (1 HP, the Shrink applier) is hit first and dies; enemy A (100 HP) is the second target.
fn thunderclap_shrink_other_hp(version: GameVersion) -> i32 {
    let mut state = fresh(version, CharacterId::Ironclad);
    let applier = add_enemy(&mut state, 2, 1);
    let other = add_enemy(&mut state, 3, 100);
    add_player_power_with_applier(&mut state, 900, PowerKind::Shrink, -1, applier);
    let i = push_hand(&mut state, CardId::Thunderclap, false);
    let _ = sts2_core::combat_loop::play_card(&mut state, i, None);
    hp_of(&state, other)
}

// ── LoseMaxHp below current HP deals damage (seed LSA_34) ──────────────────────────────────────────

/// Full-HP player with Centennial Puzzle plays Brightest Flame (draw 2, lose max HP). Returns the hand size.
fn brightest_flame_puzzle_hand(version: GameVersion) -> usize {
    let mut state = fresh(version, CharacterId::Ironclad);
    let _ = add_enemy(&mut state, 2, 100);
    state.relics.push(sts2_core::Relic {
        id: NonZeroU32::new(1).expect("nz"),
        kind: sts2_core::RelicKind::CentennialPuzzle,
        counter: 0,
        is_wax: false,
        is_melted: false,
    });
    for _ in 0..10 {
        let uid = new_card(&mut state, CardId::StrikeIronclad, false);
        state.player.draw.push(uid);
    }
    let i = push_hand(&mut state, CardId::BrightestFlame, false);
    let _ = sts2_core::combat_loop::play_card(&mut state, i, None);
    state.player.hand.len()
}

// ── Burning Sticks latch re-armed at combat end (seeds LSA_77, 42) ─────────────────────────────────

/// Exhausts one Forgotten Ritual (the content listener spends the latch and clones it into hand), ends the combat,
/// and returns the relic's latch counter.
fn burning_sticks_latch_after_combat(version: GameVersion) -> (i32, usize) {
    let mut state = fresh(version, CharacterId::Ironclad);
    let _ = add_enemy(&mut state, 2, 100);
    state.relics.push(sts2_core::Relic {
        id: NonZeroU32::new(1).expect("nz"),
        kind: sts2_core::RelicKind::BurningSticks,
        counter: 0,
        is_wax: false,
        is_melted: false,
    });
    let a = push_hand(&mut state, CardId::ForgottenRitual, false);
    let _ = sts2_core::combat_loop::play_card(&mut state, a, None);
    let clone_in_hand = state.player.hand.len(); // the clone returned by the relic
    let _ = sts2_core::relic_effects::dispatch_after_combat_end(&mut state);
    let counter = state
        .relics
        .iter()
        .find(|r| r.kind == sts2_core::RelicKind::BurningSticks)
        .map(|r| r.counter)
        .expect("relic");
    (counter, clone_in_hand)
}

// ── Draws issued after the lethal hit move no card (seeds LSA_59/71, shuffle & energy-cost counter drift) ─────────

/// Pommel Strike (9 damage, draw 1) against a 1-HP enemy with Strikes in the draw pile. Returns the hand size.
fn pommel_strike_lethal_hand(version: GameVersion) -> usize {
    let mut state = fresh(version, CharacterId::Ironclad);
    let _enemy = add_enemy(&mut state, 2, 1);
    for _ in 0..5 {
        let uid = new_card(&mut state, CardId::StrikeIronclad, false);
        state.player.draw.push(uid);
    }
    let i = push_hand(&mut state, CardId::PommelStrike, false);
    let _ = sts2_core::combat_loop::play_card(&mut state, i, Some(0));
    state.player.hand.len()
}

/// `CardPileCmd.DrawInternal` returns an empty draw when `CombatManager.IsOverOrEnding` (CardPileCmd.cs, head of
/// DrawInternal): the post-attack draw after a killing blow moves no card in C#.
#[test]
fn draw_after_lethal_hit_draws_nothing_v0111() {
    assert_eq!(pommel_strike_lethal_hand(V111), 0);
}

#[test]
fn draw_after_lethal_hit_still_draws_v0107_unchanged() {
    assert_eq!(pommel_strike_lethal_hand(V107), 1);
}
