//! [`InlineKv`] — a fixed-capacity, `Copy` key→i32 map for [`crate::Card`].
//!
//! Replaces the two `SmallVec<[(K, i32); N]>` fields (`combat_vars`,
//! `enchantments`) that were the only thing keeping `Card` from being
//! `Copy`. With `Card: Copy`, `Vec<Card>::clone` — the per-simulation
//! card-arena deep-clone in MCTS (2–3 full `CombatState` clones per
//! simulation) — specializes to a single `memcpy` instead of an
//! element-wise loop with per-element branch + drop glue. See
//! `docs/perf/2026-07-05-mcts-single-thread-speedup.md` ("Remaining
//! levers": the clone complex, with `Card: Copy` named as the untested
//! remaining option).
//!
//! # Behavior contract (load-bearing)
//!
//! This type must be observationally IDENTICAL to the `SmallVec` it
//! replaces, because `Card`'s derived `PartialEq` and its `Debug` output
//! both feed search behavior:
//!
//! * `PartialEq` participates in [`crate::cards::cards_observationally_equal`],
//!   which feeds the DrawChance segment dedup and (via
//!   `crate::hashing::card_observational_key`, which hashes
//!   `format!("{card:?}")`) the transposition/dedup hashes. Equality is
//!   therefore **insertion-order-sensitive over the live entries** —
//!   exactly `SmallVec`'s slice equality. Canonical zeroed tail slots
//!   (see below) make the derived array compare equivalent to a live-
//!   prefix compare.
//! * `Debug` must format **byte-identically** to `SmallVec`'s (a plain
//!   slice-style list of the live entries), so the Debug-string hashes
//!   are unchanged. The manual impl below prints only the live prefix.
//!
//! # Tail canonicalization invariant
//!
//! Slots at index `>= len` are ALWAYS `(K::FILL, 0)`. Every constructor
//! and mutator preserves this ([`InlineKv::new`] fills, [`InlineKv::push`]
//! writes at `len`, [`InlineKv::clear`] re-fills, and mutation of existing
//! entries goes through [`InlineKv::iter_mut`], which only exposes the
//! live prefix). This is what lets `PartialEq`/`Eq` be derived over the
//! whole fixed array without comparing garbage.
//!
//! # Capacity overflow
//!
//! [`InlineKv::push`] PANICS (loudly, naming the key and dumping the live
//! entries) when the fixed capacity is exceeded — a silent drop would be a
//! correctness bug, and spilling to the heap would forfeit `Copy`.
//! Capacities on the `Card` aliases are sized against the worst DISTINCT-KEY
//! set a card can legitimately accumulate, not the typical stack — slots are
//! never reclaimed, so "realistic" undersizes them (GH #474 hit exactly
//! that; see the alias docs in `cards.rs`). The whole validation suite
//! (nextest, parity, fuzz, full random-run sweeps) runs against these caps,
//! so an overflow shows up as a panic at the offending site, never as
//! silent state loss.

use core::fmt;

/// Key trait for [`InlineKv`]: fieldless `Copy` enums with a designated
/// fill variant for canonical tail slots. The fill variant is arbitrary
/// (any variant works — tails are also length-masked); by convention the
/// first declared variant.
pub trait InlineKvKey: Copy + Eq + fmt::Debug {
    /// Variant used to fill unused tail slots (paired with amount `0`).
    const FILL: Self;
}

/// Fixed-capacity insertion-ordered `(K, i32)` map. `Copy`. See the
/// module docs for the equality / Debug / tail-canonicalization contract.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct InlineKv<K: InlineKvKey, const N: usize> {
    len: u8,
    slots: [(K, i32); N],
}

impl<K: InlineKvKey, const N: usize> InlineKv<K, N> {
    /// Empty map (all slots canonical `(K::FILL, 0)`).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            len: 0,
            slots: [(K::FILL, 0); N],
        }
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.len as usize
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Live entries, insertion-ordered (the tail is not exposed).
    #[must_use]
    pub fn as_slice(&self) -> &[(K, i32)] {
        &self.slots[..self.len as usize]
    }

    pub fn iter(&self) -> core::slice::Iter<'_, (K, i32)> {
        self.as_slice().iter()
    }

    /// Mutable iteration over the live entries ONLY — the canonical tail
    /// is never exposed, so in-place value updates cannot break the tail
    /// invariant. (Callers can corrupt a live KEY to `FILL`; that is the
    /// same freedom `SmallVec` gave and no caller does it.)
    pub fn iter_mut(&mut self) -> core::slice::IterMut<'_, (K, i32)> {
        let len = self.len as usize;
        self.slots[..len].iter_mut()
    }

    #[must_use]
    pub fn first(&self) -> Option<&(K, i32)> {
        self.as_slice().first()
    }

    /// Append an entry. Mirrors `SmallVec::push` for the live prefix.
    ///
    /// # Panics
    /// When the fixed capacity `N` is exhausted. See the module docs —
    /// this is the deliberate overflow semantic, not a recoverable error.
    pub fn push(&mut self, kv: (K, i32)) {
        let i = self.len as usize;
        assert!(
            i < N,
            "InlineKv<{}, {N}> capacity exceeded pushing {:?} — raise the \
             capacity on the Card field alias (cards.rs) if this stacking is \
             legitimate; live entries: {:?}",
            core::any::type_name::<K>(),
            kv.0,
            self.as_slice(),
        );
        self.slots[i] = kv;
        self.len += 1;
    }

    /// Remove all entries (restores every slot to the canonical tail).
    pub fn clear(&mut self) {
        *self = Self::new();
    }
}

impl<K: InlineKvKey, const N: usize> Default for InlineKv<K, N> {
    fn default() -> Self {
        Self::new()
    }
}

/// Slice-style Debug over the LIVE entries only — must stay byte-identical
/// to the replaced `SmallVec`'s Debug output (it was hashed via
/// `format!("{card:?}")` in `hashing::card_observational_key` before
/// PR #469 moved that key to a derived field hash; keeping the format
/// identical preserves every remaining Debug-encoding consumer).
impl<K: InlineKvKey, const N: usize> fmt::Debug for InlineKv<K, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.as_slice()).finish()
    }
}

/// Slice-style Hash over the LIVE entries only — byte-identical hash
/// stream to the replaced `SmallVec`'s `Hash` (which delegates to the
/// slice: length prefix + each element). `Card` derives `Hash` and feeds
/// `hashing::card_observational_key` (PR #469's field-hash observational
/// key), so hashing the canonical tail slots — or even just a different
/// length encoding — would silently change every observational key.
impl<K: InlineKvKey + core::hash::Hash, const N: usize> core::hash::Hash for InlineKv<K, N> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.as_slice().hash(state);
    }
}

impl<K: InlineKvKey, const N: usize> FromIterator<(K, i32)> for InlineKv<K, N> {
    fn from_iter<I: IntoIterator<Item = (K, i32)>>(iter: I) -> Self {
        let mut out = Self::new();
        for kv in iter {
            out.push(kv);
        }
        out
    }
}

impl<'a, K: InlineKvKey, const N: usize> IntoIterator for &'a InlineKv<K, N> {
    type Item = &'a (K, i32);
    type IntoIter = core::slice::Iter<'a, (K, i32)>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, K: InlineKvKey, const N: usize> IntoIterator for &'a mut InlineKv<K, N> {
    type Item = &'a mut (K, i32);
    type IntoIter = core::slice::IterMut<'a, (K, i32)>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
    enum K {
        A,
        B,
        C,
    }
    impl InlineKvKey for K {
        const FILL: Self = K::A;
    }

    type M = InlineKv<K, 3>;

    #[test]
    fn push_iter_first_len() {
        let mut m = M::new();
        assert!(m.is_empty());
        m.push((K::B, 2));
        m.push((K::C, -1));
        assert_eq!(m.len(), 2);
        assert_eq!(m.first(), Some(&(K::B, 2)));
        assert_eq!(m.as_slice(), &[(K::B, 2), (K::C, -1)]);
        assert_eq!(m.iter().count(), 2);
    }

    #[test]
    fn eq_is_order_sensitive_over_live_prefix_like_smallvec() {
        let ab: M = [(K::A, 1), (K::B, 2)].into_iter().collect();
        let ba: M = [(K::B, 2), (K::A, 1)].into_iter().collect();
        let ab2: M = [(K::A, 1), (K::B, 2)].into_iter().collect();
        assert_eq!(ab, ab2);
        assert_ne!(ab, ba); // insertion order participates, as with SmallVec
                            // A live (FILL, 0) entry is NOT equal to an empty map (len differs)
        let fill0: M = [(K::A, 0)].into_iter().collect();
        assert_ne!(fill0, M::new());
    }

    #[test]
    fn iter_mut_updates_in_place_and_preserves_eq() {
        let mut m: M = [(K::B, 1)].into_iter().collect();
        for (k, v) in &mut m {
            if *k == K::B {
                *v = 7;
            }
        }
        let want: M = [(K::B, 7)].into_iter().collect();
        assert_eq!(m, want);
    }

    #[test]
    fn clear_restores_canonical_tail_so_eq_holds() {
        let mut m: M = [(K::C, 5), (K::B, 6)].into_iter().collect();
        m.clear();
        assert_eq!(m, M::new());
        // Rebuild after clear must equal a fresh build (tail canonical).
        m.push((K::C, 5));
        let fresh: M = [(K::C, 5)].into_iter().collect();
        assert_eq!(m, fresh);
    }

    #[test]
    fn debug_matches_smallvec_slice_format() {
        let m: M = [(K::B, 2), (K::C, -1)].into_iter().collect();
        // SmallVec's Debug delegates to the slice: `[(B, 2), (C, -1)]`.
        assert_eq!(format!("{m:?}"), "[(B, 2), (C, -1)]");
        assert_eq!(format!("{:?}", M::new()), "[]");
        // Alternate form must also match the slice formatter (the
        // observational-key hash uses the non-alternate form, but keep
        // parity everywhere).
        let sv: &[(K, i32)] = &[(K::B, 2), (K::C, -1)];
        assert_eq!(format!("{m:#?}"), format!("{sv:#?}"));
    }

    #[test]
    fn hash_matches_slice_hash_and_ignores_tail() {
        use core::hash::{Hash, Hasher};
        fn h(x: &impl Hash) -> u64 {
            let mut s = std::collections::hash_map::DefaultHasher::new();
            x.hash(&mut s);
            s.finish()
        }
        let m: M = [(K::B, 2), (K::C, -1)].into_iter().collect();
        let sv: &[(K, i32)] = &[(K::B, 2), (K::C, -1)];
        // Byte-identical hash stream to the slice (== SmallVec's Hash).
        assert_eq!(h(&m), h(&sv));
        // A cleared-and-refilled map hashes like a fresh one (tail never
        // participates).
        let mut n: M = [(K::A, 9), (K::B, 8), (K::C, 7)].into_iter().collect();
        n.clear();
        n.push((K::B, 2));
        n.push((K::C, -1));
        assert_eq!(h(&n), h(&m));
    }

    #[test]
    #[should_panic(expected = "capacity exceeded")]
    fn push_past_capacity_panics_loudly() {
        let mut m = M::new();
        m.push((K::A, 1));
        m.push((K::B, 2));
        m.push((K::C, 3));
        m.push((K::A, 4));
    }
}
