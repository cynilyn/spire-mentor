//! `Imbued` (`ShouldStartAtBottomOfDrawPile`): on turn 1 a card carrying the enchantment is
//! moved to the BOTTOM of the draw pile before the opening draw and auto-played from there
//! (`CombatManager.SetupPlayerTurn`, `Imbued.AfterAutoPrePlayPhaseEntered`). v0.111.0 only:
//! earlier the card was shuffled like any other and could land in the opening hand.

use sts2_core::cards::{CardId, EnchantmentId};
use sts2_core::combat_loop::{start_combat, start_player_turn};
use sts2_core::potions::CharacterId;
use sts2_core::{CardUid, CombatState, GameVersion, MonsterKind};

fn install() {
    sts2_content::cards::dispatch::install_default_registry();
    sts2_content::relics::dispatch::ensure_installed();
    sts2_content::potions::dispatch::ensure_installed();
    sts2_core::register_card_factory(sts2_content::cards::spec_index::card_pod_from_id);
}

fn add_card(state: &mut CombatState, card_id: CardId) -> CardUid {
    let mut card = sts2_core::lookup_card_factory()
        .and_then(|f| f(card_id, false))
        .expect("card factory has the card");
    let uid = CardUid::new(u32::try_from(state.cards.len() + 1).expect("fits")).expect("nz");
    card.uid = uid;
    state.cards.push(card);
    uid
}

/// Opening turn with an Imbued Defend at the TOP of a draw pile of Strikes:
/// `(imbued card is in hand, imbued card is in discard, block)`.
fn opening(version: GameVersion, seed: i32) -> (bool, bool, i32) {
    install();
    let mut state = CombatState {
        rng: sts2_core::dotnet_rng::Rng::new(seed),
        run_rng: sts2_core::run_rng_set::RunRngSet::new(seed),
        game_version: version,
        ..Default::default()
    };
    state.player.character = Some(CharacterId::Ironclad);
    state.player.current_hp = 70;
    state.player.max_hp = 70;
    state.player.max_energy = 3;
    state.init_player_creature();
    let enemy = state.spawn_enemy(MonsterKind::Nibbit);
    if let Some(c) = state.creatures.iter_mut().find(|c| c.id == enemy.0) {
        c.max_hp = 500;
        c.current_hp = 500;
    }
    let imbued = add_card(&mut state, CardId::DefendIronclad);
    state.cards[imbued.0.get() as usize - 1].add_enchantment(EnchantmentId::Imbued, 1);
    state.player.draw.push(imbued);
    for _ in 0..10 {
        let uid = add_card(&mut state, CardId::StrikeIronclad);
        state.player.draw.push(uid);
    }
    let _ = start_combat(&mut state);
    let _ = start_player_turn(&mut state);
    (
        state.player.hand.contains(&imbued),
        state.player.discard.contains(&imbued),
        state.player.block,
    )
}

#[test]
fn imbued_starts_at_the_bottom_and_auto_plays_on_v0111_only() {
    let mut earlier_drew_it = false;
    for seed in 1..40 {
        // v0.111.0: never in the opening hand, auto-played from the bottom (5 block, ends in discard).
        let (in_hand, in_discard, block) = opening(GameVersion::V0_111_0, seed);
        assert!(
            !in_hand && in_discard && block == 5,
            "seed {seed}: {in_hand} {in_discard} {block}"
        );
        // v0.107.1 is unchanged: the card is shuffled like any other, so some seeds draw it.
        earlier_drew_it |= opening(GameVersion::V0_107_1, seed).0;
    }
    assert!(earlier_drew_it);
}

/// `WhisperingEarring` (+1 max energy) plays the first playable cards of the opening hand on its own
/// (`AfterAutoPrePlayPhaseEnteredLate`); v0.111.0 only. Returns `(cards left in hand, energy left)`.
fn earring_opening(version: GameVersion) -> (usize, i32) {
    install();
    let mut state = CombatState {
        rng: sts2_core::dotnet_rng::Rng::new(7),
        run_rng: sts2_core::run_rng_set::RunRngSet::new(7),
        game_version: version,
        ..Default::default()
    };
    state.player.character = Some(CharacterId::Ironclad);
    state.player.current_hp = 70;
    state.player.max_hp = 70;
    state.player.max_energy = 3;
    state.init_player_creature();
    let enemy = state.spawn_enemy(MonsterKind::Nibbit);
    if let Some(c) = state.creatures.iter_mut().find(|c| c.id == enemy.0) {
        c.max_hp = 500;
        c.current_hp = 500;
    }
    state.relics.push(sts2_core::Relic {
        id: std::num::NonZeroU32::new(100).expect("non-zero"),
        kind: sts2_core::RelicKind::WhisperingEarring,
        counter: 0,
        is_wax: false,
        is_melted: false,
    });
    state.relics.ensure_plan();
    for _ in 0..10 {
        let uid = add_card(&mut state, CardId::DefendIronclad);
        state.player.draw.push(uid);
    }
    let _ = start_combat(&mut state);
    let _ = start_player_turn(&mut state);
    (state.player.hand.len(), i32::from(state.player.energy))
}

#[test]
fn whispering_earring_plays_the_opening_hand_on_v0111_only() {
    // v0.111.0: 4 energy, four 1-cost Defends played, one card left.
    assert_eq!(earring_opening(GameVersion::V0_111_0), (1, 0));
    // v0.107.1 is unchanged: the full hand and the energy are untouched.
    assert_eq!(earring_opening(GameVersion::V0_107_1).0, 5);
}

/// The Imbued auto-play leaves every other card's place in the draw pile alone (v0.111.0): the pile
/// after the opening turn is the shuffled pile minus the Imbued card minus the five drawn.
#[test]
fn imbued_auto_play_keeps_the_draw_order_on_v0111() {
    install();
    for seed in 1..20 {
        let mut state = CombatState {
            rng: sts2_core::dotnet_rng::Rng::new(seed),
            run_rng: sts2_core::run_rng_set::RunRngSet::new(seed),
            game_version: GameVersion::V0_111_0,
            ..Default::default()
        };
        state.player.character = Some(CharacterId::Ironclad);
        state.player.current_hp = 70;
        state.player.max_hp = 70;
        state.player.max_energy = 3;
        state.init_player_creature();
        let enemy = state.spawn_enemy(MonsterKind::Nibbit);
        if let Some(c) = state.creatures.iter_mut().find(|c| c.id == enemy.0) {
            c.max_hp = 500;
            c.current_hp = 500;
        }
        let imbued = add_card(&mut state, CardId::DefendIronclad);
        state.cards[imbued.0.get() as usize - 1].add_enchantment(EnchantmentId::Imbued, 1);
        state.player.draw.push(imbued);
        for _ in 0..12 {
            let uid = add_card(&mut state, CardId::StrikeIronclad);
            state.player.draw.push(uid);
        }
        let _ = start_combat(&mut state);
        let before: Vec<CardUid> = state
            .player
            .draw
            .iter()
            .copied()
            .filter(|u| *u != imbued)
            .collect();
        let _ = start_player_turn(&mut state);
        let expect: Vec<CardUid> = before[5..].to_vec();
        let got: Vec<CardUid> = state.player.draw.iter().copied().collect();
        assert_eq!(got, expect, "seed {seed}");
    }
}
