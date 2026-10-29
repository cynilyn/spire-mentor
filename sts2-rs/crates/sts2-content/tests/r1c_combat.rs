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

/// C# `CreatureCmd.Damage` computes every target's `ModifyDamage` before any death is processed
/// (CreatureCmd.cs:134-151, Kill at :285), and `ShrinkPower.AfterDeath` (ShrinkPower.cs:88-92) removes the power
/// only afterwards: the second target still takes the shrunk hit, floor(4 * 0.7) = 2.
#[test]
fn thunderclap_keeps_shrink_for_the_whole_attack_v0111() {
    assert_eq!(thunderclap_shrink_other_hp(V111), 98);
}

#[test]
fn thunderclap_shrink_removed_mid_attack_v0107_unchanged() {
    assert_eq!(thunderclap_shrink_other_hp(V107), 96);
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

/// `CreatureCmd.LoseMaxHp` (CreatureCmd.cs:655-658): newMaxHp < CurrentHp => Damage(Unblockable|Unpowered|Move) first,
/// so Centennial Puzzle's AfterDamageReceived (CentennialPuzzle.cs: draw 3) fires: 2 (flame) + 3 (puzzle).
#[test]
fn lose_max_hp_below_current_triggers_after_damage_listeners_v0111() {
    assert_eq!(brightest_flame_puzzle_hand(V111), 5);
}

#[test]
fn lose_max_hp_plain_shrink_v0107_unchanged() {
    assert_eq!(brightest_flame_puzzle_hand(V107), 2);
}
