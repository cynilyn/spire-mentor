//! Combat rules that changed in v0.111.0 (v0.107.1 keeps the old behaviour), on a one-enemy board.

use std::num::NonZeroU32;

use smallvec::SmallVec;
use sts2_core::cards::{
    Card, CardEnchantments, CardId, CardTags, CardType, CombatVars, TargetType,
};
use sts2_core::{
    CardUid, CombatState, Creature, CreatureUid, GameVersion, IntentHistory, MonsterId,
    MonsterKind, PowerKind,
};

use sts2_content::cards::dispatch as content_dispatch;

/// One 500-HP Nibbit and the card under test in the Play pile (uid 1), with 9 energy.
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

/// C# `CreatureCmd.Damage` skips `AfterDamageReceived` for a target killed by the
/// hit, so a Personal Hive that dies does not flood Dazed (v0.111.0 only).
#[test]
fn killing_a_personal_hive_adds_no_dazed_on_v0111_only() {
    let draw_after = |version: GameVersion, enemy_hp: i32| -> usize {
        let (mut state, enemy) = board(version, CardId::StrikeIronclad, TargetType::Enemy);
        sts2_content::monsters::act2_hive::install_personal_hive_handlers();
        sts2_core::card_effects::apply_power_to_creature(
            &mut state,
            enemy,
            PowerKind::PersonalHive,
            2,
        );
        state.creatures[0].current_hp = enemy_hp;
        sts2_core::card_effects::deal_damage_player_to_enemy(&mut state, enemy, 6);
        state.player.draw.len()
    };
    // survives the hit: the hive still floods (both versions)
    assert_eq!(draw_after(GameVersion::V0_111_0, 100), 2);
    assert_eq!(draw_after(GameVersion::V0_107_1, 100), 2);
    // killed by the hit: no flood on v0.111.0, the original behaviour before
    assert_eq!(draw_after(GameVersion::V0_111_0, 5), 0);
    assert_eq!(draw_after(GameVersion::V0_107_1, 5), 2);
}

/// `CardCmd.AutoPlay` runs `Hook.ShouldPlay`: with Normality in hand after three
/// started plays the Stampede/Havoc pick is vetoed -- no damage, no target roll
/// (v0.111.0 only).
#[test]
fn normality_vetoes_an_auto_played_attack_on_v0111_only() {
    let run = |version: GameVersion| -> (i32, u32) {
        let (mut state, _enemy) = board(version, CardId::StrikeIronclad, TargetType::Enemy);
        let strike = state.player.play.pop().expect("strike in play");
        state.player.hand.push(strike);
        let uid = CardUid::new(2).expect("non-zero uid");
        let mut normality = state.cards[0];
        normality.uid = uid;
        normality.card_id = CardId::Normality;
        normality.card_type = CardType::Curse;
        normality.target_type = TargetType::None;
        state.cards.push(normality);
        state.player.hand.push(uid);
        state.player.card_plays_started_this_turn = 3;
        let before = state.run_rng.combat_targets.counter();
        sts2_core::hooks::apply_effects(
            &mut state,
            &[sts2_core::hooks::Effect::AutoPlayRandomAttacksFromHand { count: 1 }],
        );
        (
            500 - state.creatures[0].current_hp,
            state.run_rng.combat_targets.counter() - before,
        )
    };
    assert_eq!(run(GameVersion::V0_111_0), (0, 0));
    let (dmg, rolls) = run(GameVersion::V0_107_1);
    assert!(dmg > 0 && rolls == 1, "{dmg} {rolls}");
}

/// Brilliant Scarf: the fifth card of a turn costs 0 (`TryModifyEnergyCostInCombatLate`),
/// so every hand card reads 0 once four have been played (v0.111.0).
#[test]
fn brilliant_scarf_zeroes_the_fifth_card_cost_on_v0111_only() {
    let cost = |version: GameVersion, plays: i32| -> i32 {
        let (mut state, _) = board(version, CardId::StrikeIronclad, TargetType::Enemy);
        let uid = state.player.play.pop().expect("strike in play");
        state.player.hand.push(uid);
        state.cards[0].cost = 2;
        state.relics.push(sts2_core::Relic {
            id: NonZeroU32::new(100).expect("non-zero"),
            kind: sts2_core::RelicKind::BrilliantScarf,
            counter: plays,
            is_wax: false,
            is_melted: false,
        });
        state.relics.ensure_plan();
        sts2_core::hooks::effective_card_cost(&state, uid)
    };
    assert_eq!(cost(GameVersion::V0_111_0, 4), 0);
    assert_eq!(cost(GameVersion::V0_111_0, 3), 2);
    assert_eq!(cost(GameVersion::V0_107_1, 4), 2);
}

/// `SoulsPower.OnEnchant` removes Exhaust: an enchanted Offering goes to the discard pile (v0.111.0).
#[test]
fn souls_power_removes_exhaust_on_v0111_only() {
    use sts2_core::cards::EnchantmentId;
    for (version, want) in [
        (GameVersion::V0_111_0, false),
        (GameVersion::V0_107_1, true),
    ] {
        let (mut state, _) = board(version, CardId::Offering, TargetType::Self_);
        state.cards[0].add_enchantment(EnchantmentId::SoulsPower, 1);
        assert_eq!(
            sts2_core::card_effects::card_exhausts_for(&state, &state.cards[0]),
            want,
            "{version:?}"
        );
    }
}
