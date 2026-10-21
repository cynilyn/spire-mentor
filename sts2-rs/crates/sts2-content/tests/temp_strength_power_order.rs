//! Power list order for the temporary-Strength cards.
//!
//! C# `PowerCmd.Apply` runs `TemporaryStrengthPower.BeforeApplied` (which
//! applies the companion Strength) before the marker power is added to the
//! creature's list, so the application order is [Strength, marker]. The port
//! matches that from v0.111.0 on; v0.107.1 keeps the original [marker, Strength].

use std::num::NonZeroU32;

use smallvec::SmallVec;
use sts2_core::card_effects::dispatch;
use sts2_core::cards::{
    Card, CardEnchantments, CardId, CardTags, CardType, CombatVars, TargetType,
};
use sts2_core::{
    CardUid, CombatState, Creature, CreatureUid, GameVersion, IntentHistory, MonsterId,
    MonsterKind, PowerKind,
};

use sts2_content::cards::dispatch as content_dispatch;

fn board(
    version: GameVersion,
    card_id: CardId,
    target_type: TargetType,
) -> (CombatState, CreatureUid) {
    content_dispatch::install_default_registry();
    let mut state = CombatState {
        game_version: version,
        ..CombatState::default()
    };
    let id = NonZeroU32::new(1).expect("non-zero");
    state.creatures.push(Creature {
        id,
        current_hp: 500,
        max_hp: 500,
        block: 0,
        powers: SmallVec::new(),
        species: Some(MonsterKind::Nibbit),
        intent: None,
        intent_history: IntentHistory::default(),
        monster_id: MonsterId("Nibbit"),
        ai: None,
    });
    let enemy = CreatureUid(id);
    state.enemy_uids.push(enemy);
    state.player.current_hp = 80;
    state.player.max_hp = 80;
    state.player.energy = 9;
    state.player.max_energy = 9;
    let uid = CardUid::new(1).expect("non-zero uid");
    state.cards.push(Card {
        uid,
        card_id,
        cost: 1,
        card_type: CardType::Attack,
        target_type,
        upgraded: false,
        base_damage: 6,
        base_block: 0,
        combat_vars: CombatVars::new(),
        enchantments: CardEnchantments::new(),
        tags: CardTags::NONE,
        energy_spent: 0,
        base_replay_count: 0,
        keyword_override: None,
        affliction: None,
    });
    state.player.play.push(uid);
    (state, enemy)
}

fn player_kinds(state: &CombatState) -> Vec<PowerKind> {
    state
        .player
        .powers
        .iter()
        .filter_map(|u| state.powers.iter().find(|p| p.id == u.0).map(|p| p.kind))
        .collect()
}

fn enemy_kinds(state: &CombatState) -> Vec<PowerKind> {
    state.creatures[0]
        .powers
        .iter()
        .filter_map(|u| state.powers.iter().find(|p| p.id == u.0).map(|p| p.kind))
        .collect()
}

#[test]
fn setup_strike_lists_strength_first_on_v0111_only() {
    let (mut state, enemy) = board(
        GameVersion::V0_111_0,
        CardId::SetupStrikeCard,
        TargetType::Enemy,
    );
    dispatch(&mut state, CardId::SetupStrikeCard, Some(enemy));
    assert_eq!(
        player_kinds(&state),
        vec![PowerKind::Strength, PowerKind::SetupStrike]
    );

    let (mut old, enemy) = board(
        GameVersion::V0_107_1,
        CardId::SetupStrikeCard,
        TargetType::Enemy,
    );
    dispatch(&mut old, CardId::SetupStrikeCard, Some(enemy));
    assert_eq!(
        player_kinds(&old),
        vec![PowerKind::SetupStrike, PowerKind::Strength]
    );
}

#[test]
fn mangle_lists_strength_first_on_v0111_only() {
    let (mut state, enemy) = board(GameVersion::V0_111_0, CardId::Mangle, TargetType::Enemy);
    dispatch(&mut state, CardId::Mangle, Some(enemy));
    assert_eq!(
        enemy_kinds(&state),
        vec![PowerKind::Strength, PowerKind::ManglePower]
    );

    let (mut old, enemy) = board(GameVersion::V0_107_1, CardId::Mangle, TargetType::Enemy);
    dispatch(&mut old, CardId::Mangle, Some(enemy));
    assert_eq!(
        enemy_kinds(&old),
        vec![PowerKind::ManglePower, PowerKind::Strength]
    );
}
