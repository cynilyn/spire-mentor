//! The playout net: the playout's move choice made by a small trained
//! policy instead of the hand-written priority table in [`crate::expert`].
//!
//! Every simulation scores its tree path by one playout to the end of the
//! fight, so the playout policy is the judge of every candidate move. The
//! heuristic one is an STS1-ported card-priority list that plays out its
//! whole hand every turn; in long boss fights its playouts almost never
//! win, the search gets no win signal, and the pilot plays as if the fight
//! were already lost (GH #777 human-bank audit, 2026-10-18).
//!
//! The built-in Ironclad net (`rl2`) was first fitted to the search's own
//! moves, then trained by REINFORCE on playout outcomes from the starts of
//! human A10 boss and elite fights. Inside the deployed search it raised the
//! Ironclad full-run win rate by 4.67pp pooled over A0, A3, A5, A8, A9 and
//! A10 (1,200 paired runs, McNemar p = 3.6e-4; A9 33.5% → 43.5%).
//! `docs/experiments/2026-10-18-learned-rollout-fullrun-gate.md` has the
//! numbers.
//!
//! The model is a small factorized softmax, cheap enough to run at every
//! playout step:
//!
//! ```text
//! q(s)      = W2 · relu(W1 · s + b1) + b2            (state query, D dims)
//! k(a)      = E[key(a)] + P · x(a)                    (action key, D dims)
//! logit(a)  = q(s) · k(a) + bias[key(a)] + w · x(a)
//! ```
//!
//! `s` ([`STATE_DIM`]) and `x(a)` ([`ACTION_DIM`]) are fixed feature
//! vectors computed here; `key(a)` is the card (with its upgrade), the
//! potion, `end_turn`, `choice` or `other`, so each card learns its own
//! embedding and bias. The trainer (`sts2-pycli/python/ppo/rollout_learn.py`)
//! reads the same features through the `sts2_rs.rollout_features` binding,
//! so training and playout can never disagree on them.
//!
//! Which weights a playout uses ([`net_for`]):
//! - `STS2_ROLLOUT_NET=<json>`, when set, for every character. This is the
//!   experiment path, and it fails loudly if the file does not load.
//! - Otherwise the built-in net of the fight's character
//!   (`sts2-rs/data/rollout_nets/<character>.json`, compiled in when present).
//!   Only Ironclad has one.
//! - A character with neither plays the heuristic playout unchanged.
//!
//! `STS2_ROLLOUT_TEMP` and `STS2_ROLLOUT_NO_EARLY_END` override the softmax
//! temperature and the early-EndTurn rule of the `STS2_ROLLOUT_NET` net only.
//! A built-in net always plays with the values in its file, so a variable
//! left over from an experiment cannot silently change the deployed search.
//! Each net is loaded once per process.
//!
//! A built-in net is compiled in only when its file exists
//! (`cfg(rollout_net_ironclad)`, set by `build.rs`). A checkout without the
//! file still builds and plays the heuristic playout.
#![allow(clippy::cast_precision_loss)]

use std::sync::OnceLock;

use rand::Rng;
use smallvec::SmallVec;
use sts2_core::cards::{CardId, CardType};
use sts2_core::potions::PotionKind;
use sts2_core::{Action, CharacterId, CombatState, PowerKind};

/// Width of the state feature vector `s`.
pub const STATE_DIM: usize = 32;
/// Width of the per-action feature vector `x(a)`.
pub const ACTION_DIM: usize = 24;

/// What an action's embedding row is keyed on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ActionKey {
    /// Playing this card (upgrade is part of the identity).
    Card { id: CardId, upgraded: bool },
    /// Drinking this potion.
    Potion(PotionKind),
    /// Ending the turn.
    EndTurn,
    /// Resolving a pending choice.
    Choice,
    /// Anything else (`Channel` / `Evoke`, never emitted as legal today).
    Other,
}

impl ActionKey {
    /// The key's name in the weights file and the training data.
    #[must_use]
    pub fn name(self) -> String {
        match self {
            Self::Card { id, upgraded } => {
                format!(
                    "card:{}{}",
                    id.to_python_name(),
                    if upgraded { "+" } else { "" }
                )
            }
            Self::Potion(k) => format!("potion:{}", k.python_name()),
            Self::EndTurn => "end_turn".to_owned(),
            Self::Choice => "choice".to_owned(),
            Self::Other => "other".to_owned(),
        }
    }
}

/// Per-step context shared by every action's features.
#[derive(Clone, Debug)]
pub struct StepCtx {
    /// The state feature vector `s`.
    pub state: [f32; STATE_DIM],
    max_hp: i32,
    energy: i32,
    unblocked: i32,
    strength: i32,
    player_weak: bool,
    lowest_slot: Option<u8>,
    alive: u32,
    playable: u32,
    /// Whether the enemy in each `enemy_uids` slot is Vulnerable, so an
    /// attack's lethal check reads it instead of rescanning the powers.
    enemy_vuln: SmallVec<[bool; 6]>,
}

fn clampf(x: f32, lo: f32, hi: f32) -> f32 {
    x.clamp(lo, hi)
}

/// The `(Vulnerable, Weak, Strength)` amounts on enemy creature `c`, 0 when
/// absent, in one pass over its powers.
fn creature_powers(state: &CombatState, c: &sts2_core::Creature) -> (i32, i32, i32) {
    let present = state.powers.present_kinds();
    if !(present.contains(PowerKind::Vulnerable)
        || present.contains(PowerKind::Weak)
        || present.contains(PowerKind::Strength))
    {
        return (0, 0, 0);
    }
    let (mut vuln, mut weak, mut strength) = (0, 0, 0);
    for p in c
        .powers
        .iter()
        .filter_map(|pu| state.powers.iter().find(|p| p.id == pu.0))
    {
        match p.kind {
            PowerKind::Vulnerable => vuln += p.amount,
            PowerKind::Weak => weak += p.amount,
            PowerKind::Strength => strength += p.amount,
            _ => {}
        }
    }
    (vuln, weak, strength)
}

impl StepCtx {
    /// Build the state features for `state`, whose legal actions are
    /// `actions` (only their count of card plays is read).
    #[must_use]
    pub fn new(state: &CombatState, actions: &[Action]) -> Self {
        let p = &state.player;
        let hp = p.current_hp.max(0);
        let max_hp = p.max_hp.max(1);
        let block = p.block.max(0);
        let energy = i32::from(p.energy.max(0));
        let mut incoming = 0i32;
        let (mut hp_sum, mut max_sum, mut block_sum, mut str_sum) = (0i32, 0i32, 0i32, 0i32);
        let mut min_hp = i32::MAX;
        let (mut alive, mut lowest_hp, mut lowest_slot) = (0u32, i32::MAX, None);
        let (mut any_vuln, mut any_weak, mut any_sentinel) = (false, false, false);
        let mut enemy_vuln = SmallVec::new();
        for (slot, uid) in state.enemy_uids.iter().enumerate() {
            enemy_vuln.push(false);
            let Some(c) = sts2_core::creature_by_id(&state.creatures, uid.0) else {
                continue;
            };
            if c.is_beaten_sentinel() {
                any_sentinel = true;
                max_sum += c.scored_hp(state.ascension).1;
                if let Some(i) = c.intent {
                    incoming += i.damage().saturating_mul(i32::from(i.hits()));
                }
                continue;
            }
            hp_sum += c.current_hp.max(0);
            max_sum += c.max_hp.max(0);
            let (vuln, weak, strength) = creature_powers(state, c);
            enemy_vuln[slot] = vuln > 0;
            if c.current_hp <= 0 {
                continue;
            }
            if let Some(i) = c.intent {
                incoming += i.damage().saturating_mul(i32::from(i.hits()));
            }
            alive += 1;
            block_sum += c.block.max(0);
            min_hp = min_hp.min(c.current_hp);
            if c.current_hp < lowest_hp {
                lowest_hp = c.current_hp;
                lowest_slot = u8::try_from(slot).ok();
            }
            any_vuln |= vuln > 0;
            any_weak |= weak > 0;
            str_sum += strength;
        }
        if min_hp == i32::MAX {
            min_hp = 0;
        }
        let unblocked = (incoming - block).max(0);
        let strength = state.player_power_amount(PowerKind::Strength);
        let dexterity = state.player_power_amount(PowerKind::Dexterity);
        let player_weak = state.player_power_amount(PowerKind::Weak) > 0;
        let playable = u32::try_from(
            actions
                .iter()
                .filter(|a| matches!(a, Action::PlayCard { .. }))
                .count(),
        )
        .unwrap_or(u32::MAX);
        let mhp = max_hp as f32;
        let removed = if max_sum > 0 {
            1.0 - hp_sum as f32 / max_sum as f32
        } else {
            1.0
        };
        let b = |x: bool| if x { 1.0 } else { 0.0 };
        let s = [
            hp as f32 / mhp,
            mhp / 100.0,
            clampf(block as f32 / mhp, 0.0, 2.0),
            energy as f32 / 3.0,
            f32::from(p.max_energy.max(0)) / 3.0,
            p.hand.len() as f32 / 10.0,
            p.draw.len() as f32 / 30.0,
            p.discard.len() as f32 / 30.0,
            p.exhaust.len() as f32 / 30.0,
            clampf(f32::from(state.round) / 20.0, 0.0, 2.0),
            clampf(incoming as f32 / mhp, 0.0, 3.0),
            clampf(unblocked as f32 / mhp, 0.0, 3.0),
            b(incoming > 0),
            b(incoming > 0 && unblocked >= hp),
            alive as f32 / 3.0,
            removed,
            clampf(hp_sum as f32 / 200.0, 0.0, 3.0),
            clampf(min_hp as f32 / 100.0, 0.0, 3.0),
            clampf(block_sum as f32 / 30.0, 0.0, 3.0),
            clampf(strength as f32 / 5.0, -3.0, 3.0),
            clampf(dexterity as f32 / 5.0, -3.0, 3.0),
            b(player_weak),
            b(state.player_power_amount(PowerKind::Vulnerable) > 0),
            b(state.player_power_amount(PowerKind::Frail) > 0),
            b(any_vuln),
            b(any_weak),
            clampf(str_sum as f32 / 5.0, -3.0, 3.0),
            p.potions.len() as f32 / 3.0,
            b(state.is_elite),
            b(any_sentinel),
            playable as f32 / 10.0,
            1.0,
        ];
        Self {
            state: s,
            max_hp,
            energy,
            unblocked,
            strength,
            player_weak,
            lowest_slot,
            alive,
            playable,
            enemy_vuln,
        }
    }
}

/// The embedding key and feature vector `x(a)` of one legal action.
/// `heur_priority` is the heuristic rollout's priority for it
/// ([`crate::expert::heuristic_priorities`]).
#[must_use]
pub fn action_features(
    state: &CombatState,
    ctx: &StepCtx,
    action: &Action,
    heur_priority: i16,
) -> (ActionKey, [f32; ACTION_DIM]) {
    let mut x = [0.0f32; ACTION_DIM];
    let mhp = ctx.max_hp as f32;
    x[21] = clampf(
        -f32::from(heur_priority) / crate::expert::SOFTMAX_TAU,
        -10.0,
        3.0,
    );
    let key = match action {
        Action::PlayCard { hand_index, target } => {
            let Some(card) = state
                .player
                .hand
                .get(*hand_index as usize)
                .and_then(|uid| state.cards.get(uid.0.get() as usize - 1))
            else {
                x[4] = 1.0;
                return (ActionKey::Other, x);
            };
            let cost = if card.is_x_cost() {
                ctx.energy
            } else {
                i32::from(card.cost.max(0))
            };
            x[0] = 1.0;
            x[5] = cost as f32 / 3.0;
            x[6] = if cost == 0 { 1.0 } else { 0.0 };
            x[7] = (ctx.energy - cost).max(0) as f32 / 3.0;
            match card.card_type {
                CardType::Attack => x[8] = 1.0,
                CardType::Skill => x[9] = 1.0,
                CardType::Power => x[10] = 1.0,
                CardType::Curse | CardType::Status => x[11] = 1.0,
            }
            x[12] = card.base_damage as f32 / 20.0;
            x[13] = card.base_block as f32 / 20.0;
            x[14] = if card.upgraded { 1.0 } else { 0.0 };
            x[20] = card.base_block.min(ctx.unblocked).max(0) as f32 / 20.0;
            if let Some(t) = *target {
                x[15] = 1.0;
                if let Some(c) = state
                    .enemy_uids
                    .get(usize::from(t))
                    .and_then(|u| sts2_core::creature_by_id(&state.creatures, u.0))
                {
                    if !c.is_beaten_sentinel() && c.max_hp > 0 {
                        x[16] = c.current_hp.max(0) as f32 / c.max_hp as f32;
                        x[17] = if ctx.alive > 1 && ctx.lowest_slot == Some(t) {
                            1.0
                        } else {
                            0.0
                        };
                        if let Some(i) = c.intent {
                            x[18] = clampf(
                                i.damage().saturating_mul(i32::from(i.hits())) as f32 / mhp,
                                0.0,
                                3.0,
                            );
                        }
                        if card.card_type == CardType::Attack && card.base_damage > 0 {
                            let mut dmg = (card.base_damage + ctx.strength).max(0) as f32;
                            if ctx.player_weak {
                                dmg *= 0.75;
                            }
                            if ctx.enemy_vuln.get(usize::from(t)).copied().unwrap_or(false) {
                                dmg *= 1.5;
                            }
                            x[19] = if dmg >= (c.current_hp + c.block.max(0)) as f32 {
                                1.0
                            } else {
                                0.0
                            };
                        }
                    }
                }
            }
            ActionKey::Card {
                id: card.card_id,
                upgraded: card.upgraded,
            }
        }
        Action::EndTurn => {
            x[1] = 1.0;
            x[7] = ctx.energy as f32 / 3.0;
            x[22] = ctx.playable as f32 / 10.0;
            x[23] = clampf(ctx.unblocked as f32 / mhp, 0.0, 3.0);
            ActionKey::EndTurn
        }
        Action::DrinkPotion {
            potion_slot: slot,
            target,
        }
        | Action::UsePotion { slot, target } => {
            x[2] = 1.0;
            x[7] = ctx.energy as f32 / 3.0;
            x[15] = if target.is_some() { 1.0 } else { 0.0 };
            x[23] = clampf(ctx.unblocked as f32 / mhp, 0.0, 3.0);
            match state.player.potions.get(usize::from(*slot)) {
                Some(k) => ActionKey::Potion(*k),
                None => ActionKey::Other,
            }
        }
        Action::ResolveChoice { .. } => {
            x[3] = 1.0;
            ActionKey::Choice
        }
        Action::Channel { .. } | Action::Evoke { .. } => {
            x[4] = 1.0;
            ActionKey::Other
        }
    };
    (key, x)
}

/// The playout net's weights. See the module docs for the model.
#[derive(Debug)]
pub struct RolloutNet {
    hidden: usize,
    emb_dim: usize,
    w1: Vec<f32>,
    b1: Vec<f32>,
    w2: Vec<f32>,
    b2: Vec<f32>,
    emb: Vec<f32>,
    bias: Vec<f32>,
    proj: Vec<f32>,
    lin: Vec<f32>,
    /// Softmax temperature applied to the logits.
    pub temperature: f32,
    /// When set, `EndTurn` is only sampled once no card is playable — the
    /// heuristic playout's rule (GH #446: playouts that stop early are worse
    /// evaluators).
    pub no_early_end_turn: bool,
    card_rows: Vec<u32>,
    potion_rows: Vec<(PotionKind, u32)>,
    end_turn_row: u32,
    choice_row: u32,
    other_row: u32,
    /// Number of embedding rows (row 0 = unknown key).
    pub rows: usize,
}

#[derive(serde::Deserialize)]
struct NetFile {
    state_dim: usize,
    action_dim: usize,
    hidden: usize,
    emb_dim: usize,
    temperature: f32,
    #[serde(default)]
    no_early_end_turn: bool,
    keys: Vec<String>,
    w1: Vec<Vec<f32>>,
    b1: Vec<f32>,
    w2: Vec<Vec<f32>>,
    b2: Vec<f32>,
    emb: Vec<Vec<f32>>,
    bias: Vec<f32>,
    proj: Vec<Vec<f32>>,
    lin: Vec<f32>,
}

fn flatten(m: &[Vec<f32>], rows: usize, cols: usize, what: &str) -> Result<Vec<f32>, String> {
    if m.len() != rows || m.iter().any(|r| r.len() != cols) {
        return Err(format!("{what}: expected {rows}x{cols}"));
    }
    Ok(m.iter().flatten().copied().collect())
}

impl RolloutNet {
    /// Parse a weights file (JSON written by the trainer).
    ///
    /// # Errors
    /// A dimension that disagrees with this build's features, a malformed
    /// matrix, or an unknown key name.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let f: NetFile = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if f.state_dim != STATE_DIM || f.action_dim != ACTION_DIM {
            return Err(format!(
                "feature dims {}x{} in file, {STATE_DIM}x{ACTION_DIM} in this build",
                f.state_dim, f.action_dim
            ));
        }
        let (h, d, rows) = (f.hidden, f.emb_dim, f.keys.len());
        if h == 0 || d == 0 {
            return Err("hidden and emb_dim must be positive".to_owned());
        }
        if f.keys.first().map(String::as_str) != Some("<unk>") {
            return Err("keys[0] must be \"<unk>\"".to_owned());
        }
        let card_by_name: std::collections::HashMap<&str, CardId> = CardId::all()
            .iter()
            .map(|c| (c.to_python_name(), *c))
            .collect();
        let mut card_rows: Vec<u32> = Vec::new();
        let mut potion_rows = Vec::new();
        let (mut end_turn_row, mut choice_row, mut other_row) = (0, 0, 0);
        for (row, k) in f.keys.iter().enumerate().skip(1) {
            let row = u32::try_from(row).map_err(|_| "too many keys".to_owned())?;
            if let Some(card) = k.strip_prefix("card:") {
                let (name, up) = card.strip_suffix('+').map_or((card, false), |n| (n, true));
                // Cards this build doesn't know (renamed / removed) keep row 0.
                if let Some(&id) = card_by_name.get(name) {
                    let idx = id as usize * 2 + usize::from(up);
                    if card_rows.len() <= idx {
                        card_rows.resize(idx + 1, 0);
                    }
                    card_rows[idx] = row;
                }
            } else if let Some(name) = k.strip_prefix("potion:") {
                if let Some(p) = PotionKind::ALL.iter().find(|p| p.python_name() == name) {
                    potion_rows.push((*p, row));
                }
            } else {
                match k.as_str() {
                    "end_turn" => end_turn_row = row,
                    "choice" => choice_row = row,
                    "other" => other_row = row,
                    _ => return Err(format!("unknown key {k:?}")),
                }
            }
        }
        Ok(Self {
            hidden: h,
            emb_dim: d,
            w1: flatten(&f.w1, h, STATE_DIM, "w1")?,
            b1: (f.b1.len() == h).then_some(f.b1).ok_or("b1")?,
            w2: flatten(&f.w2, d, h, "w2")?,
            b2: (f.b2.len() == d).then_some(f.b2).ok_or("b2")?,
            emb: flatten(&f.emb, rows, d, "emb")?,
            bias: (f.bias.len() == rows).then_some(f.bias).ok_or("bias")?,
            proj: flatten(&f.proj, d, ACTION_DIM, "proj")?,
            lin: (f.lin.len() == ACTION_DIM).then_some(f.lin).ok_or("lin")?,
            temperature: f.temperature,
            no_early_end_turn: f.no_early_end_turn,
            card_rows,
            potion_rows,
            end_turn_row,
            choice_row,
            other_row,
            rows,
        })
    }

    fn row(&self, key: ActionKey) -> usize {
        let r = match key {
            ActionKey::Card { id, upgraded } => self
                .card_rows
                .get(id as usize * 2 + usize::from(upgraded))
                .copied()
                .unwrap_or(0),
            ActionKey::Potion(p) => self
                .potion_rows
                .iter()
                .find(|(k, _)| *k == p)
                .map_or(0, |(_, r)| *r),
            ActionKey::EndTurn => self.end_turn_row,
            ActionKey::Choice => self.choice_row,
            ActionKey::Other => self.other_row,
        };
        r as usize
    }

    /// The per-step half of every logit: `q(s)` and `lin + Pᵀ·q(s)`.
    fn prepare(&self, s: &[f32; STATE_DIM]) -> StepQuery {
        let hid: SmallVec<[f32; 64]> = self
            .w1
            .chunks_exact(STATE_DIM)
            .zip(&self.b1)
            .map(|(row, b)| (b + dot(row, s)).max(0.0))
            .collect();
        let q: SmallVec<[f32; 32]> = self
            .w2
            .chunks_exact(self.hidden)
            .zip(&self.b2)
            .map(|(row, b)| b + dot(row, &hid))
            .collect();
        let mut w = [0.0f32; ACTION_DIM];
        w.copy_from_slice(&self.lin);
        for (q_j, p_row) in q.iter().zip(self.proj.chunks_exact(ACTION_DIM)) {
            for (w_k, p_jk) in w.iter_mut().zip(p_row) {
                *w_k += q_j * p_jk;
            }
        }
        StepQuery { q, w }
    }

    /// Logits for every legal action in `actions`.
    #[must_use]
    pub fn logits(
        &self,
        state: &CombatState,
        actions: &[Action],
        end_turn_priority: i16,
    ) -> SmallVec<[f32; 16]> {
        let ctx = StepCtx::new(state, actions);
        let heur = crate::expert::heuristic_priorities(state, actions, end_turn_priority);
        let step = self.prepare(&ctx.state);
        actions
            .iter()
            .zip(&heur)
            .map(|(action, &prio)| {
                let (key, feats) = action_features(state, &ctx, action, prio);
                self.logit(&step, self.row(key), &feats)
            })
            .collect()
    }

    /// One action's logit, `bias[row] + q·E[row] + (lin + Pᵀ·q)·x` — the
    /// model's `q·(E[row] + P·x) + bias[row] + lin·x` with the `P` product
    /// hoisted into [`Self::prepare`], once per step instead of per action.
    fn logit(&self, step: &StepQuery, row: usize, feats: &[f32; ACTION_DIM]) -> f32 {
        let emb = &self.emb[row * self.emb_dim..(row + 1) * self.emb_dim];
        self.bias[row] + dot(&step.q, emb) + dot(&step.w, feats)
    }
}

/// [`RolloutNet::prepare`]'s output for one playout step.
struct StepQuery {
    /// The state query `q(s)`.
    q: SmallVec<[f32; 32]>,
    /// The action-feature weights `lin + Pᵀ·q(s)`.
    w: [f32; ACTION_DIM],
}

/// `a · b` over eight independent partial sums, which LLVM vectorizes; a
/// single running sum is a serial chain of float adds it may not reorder.
fn dot(a: &[f32], b: &[f32]) -> f32 {
    let (ca, cb) = (a.chunks_exact(8), b.chunks_exact(8));
    let tail = ca
        .remainder()
        .iter()
        .zip(cb.remainder())
        .map(|(x, y)| x * y)
        .sum::<f32>();
    let mut acc = [0.0f32; 8];
    for (xa, xb) in ca.zip(cb) {
        for ((s, x), y) in acc.iter_mut().zip(xa).zip(xb) {
            *s += x * y;
        }
    }
    acc.iter().sum::<f32>() + tail
}

/// One recorded playout decision with at least two allowed actions (a
/// forced step carries no policy gradient).
#[derive(Clone, Debug)]
pub struct TrajStep {
    /// The state features `s`.
    pub state: [f32; STATE_DIM],
    /// Each legal action's embedding row in the net.
    pub rows: SmallVec<[u32; 16]>,
    /// Each legal action's features `x(a)`.
    pub x: SmallVec<[[f32; ACTION_DIM]; 16]>,
    /// Whether each action could be sampled (the no-early-end rule masks
    /// `EndTurn`).
    pub allowed: SmallVec<[bool; 16]>,
    /// Index of the sampled action.
    pub chosen: u8,
}

/// One playout by the net with every decision recorded, for training the
/// net on playout outcomes.
#[derive(Clone, Debug)]
pub struct Trajectory {
    /// The recorded decisions, in order.
    pub steps: Vec<TrajStep>,
    /// Whether the player won the combat.
    pub won: bool,
    /// Final HP / max HP.
    pub hp_frac: f32,
    /// Fraction of enemy HP removed (beaten sentinels count as removed).
    pub removed: f32,
    /// Final round.
    pub round: u16,
}

/// Play `state` out with `net` (sampling at `temperature`) and record every
/// decision.
pub fn playout_trajectory<R: Rng>(
    net: &RolloutNet,
    state: &CombatState,
    rng: &mut R,
    temperature: f32,
    no_early_end_turn: bool,
    max_steps: u32,
) -> Trajectory {
    let mut s = state.clone();
    let mut steps = Vec::new();
    let mut n = 0u32;
    while !sts2_core::is_terminal(&s) && n < max_steps {
        let acts = sts2_core::legal_actions(&s);
        if acts.is_empty() {
            break;
        }
        let ctx = StepCtx::new(&s, &acts);
        let heur = crate::expert::heuristic_priorities(&s, &acts, crate::expert::END_TURN_PRIORITY);
        let query = net.prepare(&ctx.state);
        let any_card = acts.iter().any(|a| matches!(a, Action::PlayCard { .. }));
        let mut step = TrajStep {
            state: ctx.state,
            rows: SmallVec::new(),
            x: SmallVec::new(),
            allowed: SmallVec::new(),
            chosen: 0,
        };
        let mut logits: SmallVec<[f32; 16]> = SmallVec::new();
        for (a, &h) in acts.iter().zip(&heur) {
            let (key, feats) = action_features(&s, &ctx, a, h);
            let row = net.row(key);
            let ok = !(no_early_end_turn && any_card && matches!(a, Action::EndTurn));
            logits.push(if ok {
                net.logit(&query, row, &feats)
            } else {
                f32::NEG_INFINITY
            });
            step.rows.push(u32::try_from(row).unwrap_or(0));
            step.x.push(feats);
            step.allowed.push(ok);
        }
        let idx = sample(rng, &logits, temperature);
        if step.allowed.iter().filter(|ok| **ok).count() >= 2 {
            step.chosen = u8::try_from(idx).unwrap_or(u8::MAX);
            steps.push(step);
        }
        let _ = sts2_core::apply(&mut s, acts[idx].clone());
        n += 1;
    }
    let max_sum = crate::sum_enemy_max_hp(&s);
    Trajectory {
        steps,
        won: s.player_won,
        hp_frac: s.player.current_hp.max(0) as f32 / s.player.max_hp.max(1) as f32,
        removed: if max_sum > 0 {
            1.0 - crate::sum_enemy_hp(&s) as f32 / max_sum as f32
        } else {
            1.0
        },
        round: s.round,
    }
}

/// [`playout_trajectory`] for many `(state, seed)` jobs in parallel.
#[must_use]
pub fn playout_trajectories(
    net: &RolloutNet,
    jobs: &[(CombatState, u64)],
    temperature: f32,
    no_early_end_turn: bool,
    max_steps: u32,
) -> Vec<Trajectory> {
    use rand::SeedableRng;
    use rayon::prelude::*;
    jobs.par_iter()
        .map(|(st, seed)| {
            let mut rng = rand_xoshiro::Xoshiro256PlusPlus::seed_from_u64(*seed);
            playout_trajectory(net, st, &mut rng, temperature, no_early_end_turn, max_steps)
        })
        .collect()
}

static NET: OnceLock<Option<RolloutNet>> = OnceLock::new();

/// The net named by `STS2_ROLLOUT_NET`, loaded on first use; `None` when the
/// variable is unset. When set it replaces the built-in nets for every
/// character ([`net_for`]).
///
/// # Panics
/// When `STS2_ROLLOUT_NET` names a file that can't be read or parsed — a
/// silent fallback would quietly measure another policy instead.
pub fn global_net() -> Option<&'static RolloutNet> {
    NET.get_or_init(|| {
        let path = std::env::var("STS2_ROLLOUT_NET").ok()?;
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("STS2_ROLLOUT_NET={path}: {e}"));
        let net =
            RolloutNet::from_json(&text).unwrap_or_else(|e| panic!("STS2_ROLLOUT_NET={path}: {e}"));
        Some(with_env_overrides(net))
    })
    .as_ref()
}

/// Install `net` as the process-wide policy instead of loading
/// `STS2_ROLLOUT_NET`. Returns `false` (dropping `net`) when the policy was
/// already resolved.
pub fn set_global_net(net: RolloutNet) -> bool {
    NET.set(Some(net)).is_ok()
}

/// `net` with `STS2_ROLLOUT_NO_EARLY_END` and `STS2_ROLLOUT_TEMP` applied.
/// Only the experiment net ([`global_net`]) takes them.
///
/// # Panics
/// When `STS2_ROLLOUT_TEMP` is not a positive number: ignoring a typo would
/// run the file's temperature while the experiment believes otherwise.
fn with_env_overrides(mut net: RolloutNet) -> RolloutNet {
    if let Ok(v) = std::env::var("STS2_ROLLOUT_NO_EARLY_END") {
        net.no_early_end_turn = matches!(v.as_str(), "1" | "true");
    }
    if let Ok(v) = std::env::var("STS2_ROLLOUT_TEMP") {
        net.temperature = v
            .parse::<f32>()
            .ok()
            .filter(|t| t.is_finite() && *t > 0.0)
            .unwrap_or_else(|| panic!("STS2_ROLLOUT_TEMP={v:?} is not a positive number"));
    }
    net
}

/// The built-in Ironclad net. `build.rs` sets `rollout_net_ironclad` only
/// when the file exists, so a checkout without it still compiles.
#[cfg(rollout_net_ironclad)]
const IRONCLAD_NET_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../data/rollout_nets/ironclad.json"
));

/// The built-in net for `character`, parsed on first use. `None` for a
/// character without one: every character but Ironclad.
///
/// # Panics
/// When the compiled-in net does not parse. It is a build-time asset, so
/// the `builtin_ironclad_net_loads` test catches that first.
#[cfg(rollout_net_ironclad)]
#[must_use]
pub fn builtin_net(character: CharacterId) -> Option<&'static RolloutNet> {
    static IRONCLAD: OnceLock<RolloutNet> = OnceLock::new();
    (character == CharacterId::Ironclad).then(|| {
        IRONCLAD.get_or_init(|| {
            RolloutNet::from_json(IRONCLAD_NET_JSON)
                .unwrap_or_else(|e| panic!("built-in Ironclad rollout net: {e}"))
        })
    })
}

/// The built-in net for `character`: always `None` in this build, which has
/// no `sts2-rs/data/rollout_nets/ironclad.json`.
#[cfg(not(rollout_net_ironclad))]
#[must_use]
pub fn builtin_net(_character: CharacterId) -> Option<&'static RolloutNet> {
    None
}

/// The net a playout of `state` uses: [`global_net`] when `STS2_ROLLOUT_NET`
/// is set, else the [`builtin_net`] of the fight's character. `None` means
/// the heuristic playout.
#[must_use]
pub fn net_for(state: &CombatState) -> Option<&'static RolloutNet> {
    global_net().or_else(|| state.player.character.and_then(builtin_net))
}

/// Pick a playout action at `state` with [`net_for`]'s net. When there is
/// none, use the heuristic playout ([`crate::expert::pick_rollout_action_with`])
/// with the same arguments and random draws, so a search under
/// [`crate::RolloutPolicyKind::Net`] for a character without a net is
/// identical to one under [`crate::RolloutPolicyKind::Heuristic`].
pub fn pick<R: Rng>(
    rng: &mut R,
    state: &CombatState,
    actions: &[Action],
    end_turn_priority: i16,
) -> usize {
    match net_for(state) {
        Some(net) => pick_with(
            net,
            rng,
            state,
            actions,
            end_turn_priority,
            net.temperature,
            net.no_early_end_turn,
        ),
        None => crate::expert::pick_rollout_action_with(rng, state, actions, end_turn_priority),
    }
}

/// [`pick`] with an explicit net, temperature and early-end rule.
pub fn pick_with<R: Rng>(
    net: &RolloutNet,
    rng: &mut R,
    state: &CombatState,
    actions: &[Action],
    end_turn_priority: i16,
    temperature: f32,
    no_early_end_turn: bool,
) -> usize {
    let mut logits = net.logits(state, actions, end_turn_priority);
    if no_early_end_turn && actions.iter().any(|a| matches!(a, Action::PlayCard { .. })) {
        for (l, a) in logits.iter_mut().zip(actions) {
            if matches!(a, Action::EndTurn) {
                *l = f32::NEG_INFINITY;
            }
        }
    }
    sample(rng, &logits, temperature)
}

/// Sample an index from `softmax(logits / temperature)`. A masked action
/// (`-inf` logit, weight 0) is never returned while any other action has
/// weight: an edge draw of exactly 0 or float drift past the last bucket
/// would otherwise land on it, and `rollout_learn rl` turns one such step
/// into a `-1e9` log-probability that wrecks the weights.
fn sample<R: Rng>(rng: &mut R, logits: &[f32], temperature: f32) -> usize {
    let t = temperature.max(1e-3);
    let m = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    if !m.is_finite() {
        return rng.gen_range(0..logits.len());
    }
    let w: SmallVec<[f32; 16]> = logits.iter().map(|l| ((l - m) / t).exp()).collect();
    let total: f32 = w.iter().sum();
    if !total.is_finite() || total <= 0.0 {
        return rng.gen_range(0..logits.len());
    }
    let mut x = rng.gen::<f32>() * total;
    let mut last = 0;
    for (i, &wi) in w.iter().enumerate() {
        if wi <= 0.0 {
            continue;
        }
        if x < wi {
            return i;
        }
        x -= wi;
        last = i;
    }
    last
}

/// Which policy plays a [`playout`].
#[derive(Clone, Copy, Debug)]
pub enum PlayoutPolicy {
    /// The heuristic playout ([`crate::expert::pick_rollout_action_with`]).
    Heuristic,
    /// The playout net at this temperature, with or without the
    /// no-early-end-turn rule.
    Net {
        temperature: f32,
        no_early_end_turn: bool,
    },
}

/// Play `state` to the end of the combat (or `max_steps` actions) with one
/// policy and no search — the playout the search would run from here,
/// measured on its own. Returns the final state.
///
/// # Panics
/// [`PlayoutPolicy::Net`] when [`net_for`] has no net for `state`.
pub fn playout<R: Rng>(
    state: &CombatState,
    policy: PlayoutPolicy,
    rng: &mut R,
    max_steps: u32,
) -> (CombatState, u32) {
    let mut s = state.clone();
    let mut steps = 0u32;
    while !sts2_core::is_terminal(&s) && steps < max_steps {
        let acts = sts2_core::legal_actions(&s);
        if acts.is_empty() {
            break;
        }
        let idx = match policy {
            PlayoutPolicy::Heuristic => crate::expert::pick_rollout_action_with(
                rng,
                &s,
                &acts,
                crate::expert::END_TURN_PRIORITY,
            ),
            PlayoutPolicy::Net {
                temperature,
                no_early_end_turn,
            } => pick_with(
                net_for(&s).expect("PlayoutPolicy::Net: no STS2_ROLLOUT_NET and no built-in net for this character"),
                rng,
                &s,
                &acts,
                crate::expert::END_TURN_PRIORITY,
                temperature,
                no_early_end_turn,
            ),
        };
        let _ = sts2_core::apply(&mut s, acts[idx].clone());
        steps += 1;
    }
    (s, steps)
}

/// Features of every legal action at `state`, for the training data
/// (`sts2_rs.rollout_features`): `(s, [(action, key, x(a))])`, in
/// [`sts2_core::legal_actions`] order — the order the playout sees.
#[must_use]
pub fn export_features(
    state: &CombatState,
) -> (
    [f32; STATE_DIM],
    Vec<(Action, ActionKey, [f32; ACTION_DIM])>,
) {
    let actions = sts2_core::legal_actions(state);
    let ctx = StepCtx::new(state, &actions);
    let heur =
        crate::expert::heuristic_priorities(state, &actions, crate::expert::END_TURN_PRIORITY);
    let rows = actions
        .iter()
        .zip(&heur)
        .map(|(a, &h)| {
            let (k, x) = action_features(state, &ctx, a, h);
            (a.clone(), k, x)
        })
        .collect();
    (ctx.state, rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_net(keys: &[&str], bias: &[f32]) -> String {
        let (h, d) = (2usize, 2usize);
        let zeros = |r: usize, c: usize| vec![vec![0.0f32; c]; r];
        serde_json::json!({
            "state_dim": STATE_DIM, "action_dim": ACTION_DIM, "hidden": h, "emb_dim": d,
            "temperature": 1.0, "keys": keys,
            "w1": zeros(h, STATE_DIM), "b1": vec![0.0; h], "w2": zeros(d, h), "b2": vec![0.0; d],
            "emb": zeros(keys.len(), d), "bias": bias, "proj": zeros(d, ACTION_DIM),
            "lin": vec![0.0; ACTION_DIM],
        })
        .to_string()
    }

    #[test]
    fn keys_resolve_to_their_rows() {
        let net = RolloutNet::from_json(&tiny_net(
            &[
                "<unk>",
                "end_turn",
                "card:STRIKE_IRONCLAD",
                "card:STRIKE_IRONCLAD+",
                "potion:FIRE_POTION",
            ],
            &[0.0, 1.0, 2.0, 3.0, 4.0],
        ))
        .expect("parses");
        assert_eq!(net.row(ActionKey::EndTurn), 1);
        assert_eq!(
            net.row(ActionKey::Card {
                id: CardId::StrikeIronclad,
                upgraded: false
            }),
            2
        );
        assert_eq!(
            net.row(ActionKey::Card {
                id: CardId::StrikeIronclad,
                upgraded: true
            }),
            3
        );
        assert_eq!(net.row(ActionKey::Potion(PotionKind::FirePotion)), 4);
        assert_eq!(
            net.row(ActionKey::Card {
                id: CardId::Bash,
                upgraded: false
            }),
            0,
            "unseen card"
        );
        assert_eq!(net.row(ActionKey::Choice), 0, "absent special key");
    }

    #[test]
    fn rejects_mismatched_dims_and_unknown_keys() {
        let bad = tiny_net(&["<unk>", "bogus"], &[0.0, 0.0]);
        assert!(RolloutNet::from_json(&bad)
            .unwrap_err()
            .contains("unknown key"));
        let dims = tiny_net(&["<unk>"], &[0.0])
            .replace(&format!("\"state_dim\":{STATE_DIM}"), "\"state_dim\":3");
        assert!(RolloutNet::from_json(&dims)
            .unwrap_err()
            .contains("feature dims"));
    }

    #[test]
    fn key_names_round_trip_through_the_loader() {
        let k = ActionKey::Card {
            id: CardId::Bash,
            upgraded: true,
        };
        assert_eq!(k.name(), "card:BASH+");
        assert_eq!(
            ActionKey::Potion(PotionKind::FirePotion).name(),
            "potion:FIRE_POTION"
        );
    }

    #[test]
    fn logits_match_the_model_formula() {
        use rand::{Rng, SeedableRng};
        let mut rng = rand_xoshiro::Xoshiro256PlusPlus::seed_from_u64(3);
        // Widths that aren't multiples of 8 exercise `dot`'s tail.
        let (hidden, dim) = (11usize, 3usize);
        let keys = ["<unk>", "end_turn", "card:STRIKE_IRONCLAD"];
        let mut mat = |rows: usize, cols: usize| -> Vec<Vec<f32>> {
            (0..rows)
                .map(|_| (0..cols).map(|_| rng.gen_range(-1.0f32..1.0)).collect())
                .collect()
        };
        let (w1, b1) = (mat(hidden, STATE_DIM), mat(1, hidden).remove(0));
        let (w2, b2) = (mat(dim, hidden), mat(1, dim).remove(0));
        let (emb, bias) = (mat(keys.len(), dim), mat(1, keys.len()).remove(0));
        let (proj, lin) = (mat(dim, ACTION_DIM), mat(1, ACTION_DIM).remove(0));
        let state = mat(1, STATE_DIM).remove(0);
        let feats = mat(1, ACTION_DIM).remove(0);
        let net = RolloutNet::from_json(
            &serde_json::json!({
                "state_dim": STATE_DIM, "action_dim": ACTION_DIM, "hidden": hidden,
                "emb_dim": dim, "temperature": 1.0, "keys": keys, "w1": w1, "b1": b1,
                "w2": w2, "b2": b2, "emb": emb, "bias": bias, "proj": proj, "lin": lin,
            })
            .to_string(),
        )
        .expect("parses");
        let naive_dot = |u: &[f32], v: &[f32]| u.iter().zip(v).map(|(a, b)| a * b).sum::<f32>();
        let hid: Vec<f32> = w1
            .iter()
            .zip(&b1)
            .map(|(row, b)| (b + naive_dot(row, &state)).max(0.0))
            .collect();
        let query: Vec<f32> = w2
            .iter()
            .zip(&b2)
            .map(|(row, b)| b + naive_dot(row, &hid))
            .collect();
        let step = net.prepare(&state.clone().try_into().expect("STATE_DIM"));
        let feat_arr: [f32; ACTION_DIM] = feats.clone().try_into().expect("ACTION_DIM");
        for row in 0..keys.len() {
            let want = bias[row]
                + naive_dot(&lin, &feats)
                + query
                    .iter()
                    .zip(&emb[row])
                    .zip(&proj)
                    .map(|((q_j, e_j), p_j)| q_j * (e_j + naive_dot(p_j, &feats)))
                    .sum::<f32>();
            let got = net.logit(&step, row, &feat_arr);
            assert!((want - got).abs() < 1e-4, "row {row}: {want} vs {got}");
        }
    }

    /// An `RngCore` that yields one fixed word, to pin `gen::<f32>()` at the
    /// ends of `[0, 1)`.
    struct ConstRng(u64);

    impl rand::RngCore for ConstRng {
        #[allow(clippy::cast_possible_truncation)]
        fn next_u32(&mut self) -> u32 {
            self.0 as u32
        }
        fn next_u64(&mut self) -> u64 {
            self.0
        }
        fn fill_bytes(&mut self, dest: &mut [u8]) {
            dest.fill(self.0 as u8);
        }
        fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
            self.fill_bytes(dest);
            Ok(())
        }
    }

    #[test]
    fn sampling_never_returns_a_masked_action() {
        let ninf = f32::NEG_INFINITY;
        for word in [0, u64::MAX] {
            for logits in [[ninf, 0.0, 1.0], [0.0, 1.0, ninf], [ninf, 0.0, ninf]] {
                let i = sample(&mut ConstRng(word), &logits, 1.0);
                assert!(logits[i].is_finite(), "draw {word:#x}, {logits:?} -> {i}");
            }
        }
    }

    #[test]
    fn sampling_follows_the_logits() {
        use rand::SeedableRng;
        let mut rng = rand_xoshiro::Xoshiro256PlusPlus::seed_from_u64(7);
        let logits = [0.0f32, 3.0, -50.0];
        let mut counts = [0u32; 3];
        for _ in 0..4000 {
            counts[sample(&mut rng, &logits, 1.0)] += 1;
        }
        // p = softmax([0, 3, -50]) ≈ [0.047, 0.953, 0].
        assert!(
            counts[1] > 3700 && counts[0] > 100 && counts[2] == 0,
            "{counts:?}"
        );
    }
}
