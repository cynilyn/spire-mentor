//! Card-cost changes that last for a turn or follow a clone (v0.111.0); v0.107.1 keeps its old costs.

use std::num::NonZeroU32;

use smallvec::SmallVec;
use sts2_core::card_effects::dispatch;
use sts2_core::cards::{
    Card, CardEnchantments, CardId, CardTags, CardType, CombatVars, TargetType,
};
use sts2_core::{
    CardUid, CombatState, Creature, CreatureUid, GameVersion, IntentHistory, MonsterId, MonsterKind,
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

/// The plain Enlightenment lowers hand costs only for the turn (`SetThisTurnOrUntilPlayed`):
/// the cost is back at the next turn start. v0.107.1 left the lowered cost in place.
#[test]
fn plain_enlightenment_cost_cap_expires_on_v0111_only() {
    for (version, after_restore) in [(GameVersion::V0_111_0, 3), (GameVersion::V0_107_1, 1)] {
        let (mut state, _) = board(version, CardId::Enlightenment, TargetType::Self_);
        let hand = {
            let mut c = state.cards[0];
            c.uid = CardUid::new(2).expect("non-zero uid");
            c.card_id = CardId::StrikeIronclad;
            c.card_type = CardType::Attack;
            c.target_type = TargetType::Enemy;
            c.cost = 3;
            c
        };
        let uid = hand.uid;
        state.cards.push(hand);
        state.player.hand.push(uid);
        dispatch(&mut state, CardId::Enlightenment, None);
        assert_eq!(state.cards[1].cost, 1, "{version:?}");
        state.restore_snecko_costs_for_turn_start();
        assert_eq!(state.cards[1].cost, after_restore, "{version:?}");
    }
}
