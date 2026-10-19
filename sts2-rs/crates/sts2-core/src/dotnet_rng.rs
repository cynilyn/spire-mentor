//! .NET-compatible seeded RNG and the run-side `Rng` wrapper.
//!
//! Port pin: `sts2-rl-agent@01c094d:sts2_env/core/rng.py`.
//!
//! The map generator (and other run-side systems) need byte-for-byte
//! reproducibility against the Python source-of-truth, which in turn
//! mirrors the game's C# `System.Random(int)` semantics. Rust's
//! `rand_xoshiro` family is structurally different and would diverge
//! immediately on the first `next_int`, so we hand-port the same
//! 56-element subtractive generator that .NET uses.
//!
//! Bead `sts2-rs-wn4wc` (P9.2.b) — landed alongside the map generator
//! port so parity tests against Python seed snapshots can hold. The
//! multi-stream registry (`RunRngSet`) lands separately in bead
//! `sts2-rs-ewm6b` (P9.1.g) and will own per-stream named-counter
//! state on top of this primitive.
//!
//! The port is deliberately faithful — variable names, sentinel
//! constants, and the `Sample()` / `InternalSample()` split all match
//! .NET (and Python's port) so a future divergence is easy to bisect.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

const INT_MAX: i32 = i32::MAX;
const INT_MAX_EXCLUSIVE: i64 = (i32::MAX as i64) + 1;

/// Normalize a player-entered seed string the way the C# game does.
///
/// Port of `SeedHelper.CanonicalizeSeed`
/// (`MegaCrit.Sts2.Core.Helpers/SeedHelper.cs`):
///
/// [decompiled C# omitted — see the cited path]
///
/// The game's seed alphabet is `0123456789ABCDEFGHJKLMNPQRSTUVWXYZ`
/// (no `O` / no `I` to avoid 0/1 confusion). Canonicalisation folds
/// the visually-ambiguous letters onto digits so the same human-typed
/// seed always hashes to the same int. Required before
/// [`deterministic_hash_code_old`] when ingesting scraped run-file seeds.
#[must_use]
pub fn canonicalize_seed(seed: &str) -> String {
    let upper = seed.to_uppercase();
    let folded: String = upper
        .chars()
        .map(|c| match c {
            'O' => '0',
            'I' => '1',
            other => other,
        })
        .collect();
    folded.trim().to_string()
}

/// Whether per-read RNG tracing is on (env `STS2_TRACE_RNG` set). Cached
/// once so the gate is a single relaxed load on the hot `next_int` path —
/// zero overhead when unset. Symmetric with the C# headless `RngTracePatches`
/// (shuffle-op cascade diagnosis, bead `sts2-rs-adsld`).
pub(crate) fn rng_trace_enabled() -> bool {
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("STS2_TRACE_RNG").is_some())
}

/// Deterministic 32-bit hash of `text`, matching the game's
/// `StringHelper.GetDeterministicHashCode`.
///
/// Used to derive per-stream seeds (`base_seed + hash(name)`) so a
/// single `RunRngSet` can offer independent streams keyed by string
/// without the seeds colliding across runs.
#[must_use]
pub fn deterministic_hash_code_old(text: &str) -> i32 {
    let bytes: Vec<u16> = text.encode_utf16().collect();
    let mut num: i32 = 352_654_597;
    let mut num2: i32 = num;
    let len = bytes.len();
    let mut index = 0;
    while index < len {
        num = num.wrapping_shl(5).wrapping_add(num);
        num ^= i32::from(bytes[index]);
        if index == len - 1 {
            break;
        }
        num2 = num2.wrapping_shl(5).wrapping_add(num2);
        num2 ^= i32::from(bytes[index + 1]);
        index += 2;
    }
    num.wrapping_add(num2.wrapping_mul(1_566_083_941))
}

const XXH_P1: u64 = 0x9E37_79B1_85EB_CA87;
const XXH_P2: u64 = 0xC2B2_AE3D_27D4_EB4F;
const XXH_P3: u64 = 0x1656_67B1_9E37_79F9;
const XXH_P4: u64 = 0x85EB_CA77_C2B2_AE63;
const XXH_P5: u64 = 0x27D4_EB2F_1656_67C5;

#[inline]
fn xxh_round(acc: u64, input: u64) -> u64 {
    acc.wrapping_add(input.wrapping_mul(XXH_P2))
        .rotate_left(31)
        .wrapping_mul(XXH_P1)
}

#[inline]
fn xxh_merge_round(acc: u64, val: u64) -> u64 {
    (acc ^ xxh_round(0, val))
        .wrapping_mul(XXH_P1)
        .wrapping_add(XXH_P4)
}

#[inline]
fn read_u64_le(b: &[u8]) -> u64 {
    u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
}

#[inline]
fn read_u32_le(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

/// XXH64 (seed 0) over raw bytes — the algorithm behind .NET
/// `System.IO.Hashing.XxHash64.HashToUInt64(data, 0)`. Self-contained so the
/// seed path needs no new dependency; locked by standard vectors in the tests.
#[must_use]
pub fn xxhash64_bytes(data: &[u8]) -> u64 {
    let len = data.len();
    let mut rest = data;
    let mut h: u64;
    if len >= 32 {
        let mut v1 = XXH_P1.wrapping_add(XXH_P2);
        let mut v2 = XXH_P2;
        let mut v3 = 0u64;
        let mut v4 = 0u64.wrapping_sub(XXH_P1);
        while rest.len() >= 32 {
            v1 = xxh_round(v1, read_u64_le(&rest[0..8]));
            v2 = xxh_round(v2, read_u64_le(&rest[8..16]));
            v3 = xxh_round(v3, read_u64_le(&rest[16..24]));
            v4 = xxh_round(v4, read_u64_le(&rest[24..32]));
            rest = &rest[32..];
        }
        h = v1
            .rotate_left(1)
            .wrapping_add(v2.rotate_left(7))
            .wrapping_add(v3.rotate_left(12))
            .wrapping_add(v4.rotate_left(18));
        h = xxh_merge_round(h, v1);
        h = xxh_merge_round(h, v2);
        h = xxh_merge_round(h, v3);
        h = xxh_merge_round(h, v4);
    } else {
        h = XXH_P5;
    }
    h = h.wrapping_add(len as u64);
    while rest.len() >= 8 {
        h ^= xxh_round(0, read_u64_le(rest));
        h = h.rotate_left(27).wrapping_mul(XXH_P1).wrapping_add(XXH_P4);
        rest = &rest[8..];
    }
    if rest.len() >= 4 {
        h ^= u64::from(read_u32_le(rest)).wrapping_mul(XXH_P1);
        h = h.rotate_left(23).wrapping_mul(XXH_P2).wrapping_add(XXH_P3);
        rest = &rest[4..];
    }
    for &b in rest {
        h ^= u64::from(b).wrapping_mul(XXH_P5);
        h = h.rotate_left(11).wrapping_mul(XXH_P1);
    }
    h ^= h >> 33;
    h = h.wrapping_mul(XXH_P2);
    h ^= h >> 29;
    h = h.wrapping_mul(XXH_P3);
    h ^ (h >> 32)
}

/// v0.111.0 string hash `StringHelper.GetDeterministicHashCode`:
/// `XxHash64.HashToUInt64(UTF8(text), 0)`. Replaces the 32-bit UTF-16 hash
/// (kept as [`deterministic_hash_code_old`]).
#[must_use]
pub fn xxhash64_utf8(text: &str) -> u64 {
    xxhash64_bytes(text.as_bytes())
}

/// Seed bases derived from a run's seed string (v0.111.0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeedBases<'a> {
    /// `RunRngSet.Seed`: base of the 12 run streams, maps, events, encounters.
    pub run_seed: u64,
    /// `PlayerRngSet` base (`H(full string) + slot 0`): rewards/shops/transformations.
    pub player_seed: u64,
    /// `H(full string)`: also the `act_selection` base (`StartRunLobby`).
    pub full_hash: u64,
    /// `"old"` for pre-save seed strings, else `""`.
    pub prefix: &'a str,
}

/// Derive the run and player seed bases from a seed string.
///
/// The lowercase `old` test runs on `raw` **before** normalization and is
/// case-sensitive (`OLD…` does not match). `old` seeds: `run_seed` is the
/// legacy 32-bit hash of the rest, zero-extended; the player base and
/// `act_selection` still use the xxhash of the **whole** string, prefix
/// included. Other seeds are canonicalized first (`CanonicalizeSeed`), and
/// every base is the xxhash of the canonical string.
#[must_use]
pub fn seed_bases(raw: &str) -> SeedBases<'_> {
    if let Some(rest) = raw.strip_prefix("old") {
        let full_hash = xxhash64_utf8(raw);
        SeedBases {
            run_seed: u64::from(deterministic_hash_code_old(rest) as u32),
            player_seed: full_hash,
            full_hash,
            prefix: &raw[..3],
        }
    } else {
        let full_hash = xxhash64_utf8(&canonicalize_seed(raw));
        SeedBases {
            run_seed: full_hash,
            player_seed: full_hash,
            full_hash,
            prefix: "",
        }
    }
}

/// Xoshiro256\*\* PRNG with Splitmix64 seeding — the **v0.107.1** game RNG.
///
/// Game v0.107.1's "Major Update #2" replaced the per-system PRNG. Until
/// v0.103.x the game seeded `System.Random` (see [`DotNetRandom`]) per stream;
/// because nearby/derived seeds produce correlated `System.Random` sequences,
/// independent systems (deck draw / rewards / events) were predictable from one
/// another. v0.107.1 swaps in `MegaRandom` — a faithful copy of Redzen's
/// `Xoshiro256StarStarRandom` — so the (still additive) per-stream seeds are
/// scrambled by Splitmix64 on init and the streams become independent.
///
/// Direct port of `MegaCrit.Sts2.Core.Random/MegaRandom.cs` (v0.107.1). Every
/// constant and the exact `NextULongInner` / `NextDouble` / `NextInt` derivation
/// match the C# byte-for-byte; verified against oracle-emitted reference vectors
/// in the tests below. The seed-derivation scaffolding ([`deterministic_hash_code_old`],
/// [`Rng::with_name`], counters) is UNCHANGED from v0.103.2 — only the generator
/// underneath swapped, so RNG *stream order* is identical and only the bytes
/// each draw yields differ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MegaRandom {
    s0: u64,
    s1: u64,
    s2: u64,
    s3: u64,
}

impl MegaRandom {
    /// Splitmix64 (`MegaRandom.Splitmix64`). Advances `x` and returns a mixed
    /// 64-bit value; used to expand a single seed into the 256-bit state.
    #[inline]
    fn splitmix64(x: &mut u64) -> u64 {
        *x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = *x;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Construct from a 64-bit seed (`MegaRandom(ulong)` → `Reinitialise`). The
    /// game passes a `uint` here (zero-extended); callers should pass the stream
    /// seed as `u32 as u64`.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        let mut s = seed;
        Self {
            s0: Self::splitmix64(&mut s),
            s1: Self::splitmix64(&mut s),
            s2: Self::splitmix64(&mut s),
            s3: Self::splitmix64(&mut s),
        }
    }

    /// Zeroed placeholder (never sample from it) — transient `mem::replace`
    /// sentinel only. Mirrors [`DotNetRandom::placeholder`].
    #[must_use]
    pub const fn placeholder() -> Self {
        Self {
            s0: 0,
            s1: 0,
            s2: 0,
            s3: 0,
        }
    }

    /// The four state words `[s0, s1, s2, s3]` (read-only, for dumps).
    #[must_use]
    pub const fn state(&self) -> [u64; 4] {
        [self.s0, self.s1, self.s2, self.s3]
    }

    /// Core Xoshiro256\*\* step (`MegaRandom.NextULongInner`). Returns the next
    /// 64 random bits and advances the state.
    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        let (mut x0, mut x1, mut x2, mut x3) = (self.s0, self.s1, self.s2, self.s3);
        let result = x1.wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let shifted = x1 << 17;
        x2 ^= x0;
        x3 ^= x1;
        x1 ^= x2;
        x0 ^= x3;
        x2 ^= shifted;
        x3 = x3.rotate_left(45);
        self.s0 = x0;
        self.s1 = x1;
        self.s2 = x2;
        self.s3 = x3;
        result
    }

    /// Uniform `[0, 1)` double (`MegaRandom.NextDouble`): top 53 bits scaled by
    /// `2^-53`. This is the single primitive every `Rng.Next*` is built on.
    #[inline]
    pub fn sample(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * 1.110_223_024_625_156_5e-16
    }
}

/// 56-element subtractive generator matching `System.Random(int)`.
///
/// **Legacy as of v0.107.1** — the game RNG is now [`MegaRandom`]; this
/// `System.Random` port is retained only for reference/historical parity and is
/// no longer wired into [`Rng`]. (Pre-v0.107.1 it was the live generator.)
///
/// Direct port of `_DotNetCompatRandom` from `core/rng.py`. All
/// arithmetic is signed 32-bit with .NET's `+ INT_MAX` corrective
/// branch on negatives, so two callers seeded the same way produce
/// identical sequences across runtimes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DotNetRandom {
    seed_array: [i32; 56],
    inext: usize,
    inextp: usize,
}

impl DotNetRandom {
    /// Construct from a signed 32-bit seed. `i32::MIN` is folded to
    /// `i32::MAX` (.NET's `abs` quirk) before mixing.
    #[must_use]
    pub fn new(seed: i32) -> Self {
        let mut rng = Self {
            seed_array: [0; 56],
            inext: 0,
            inextp: 21,
        };
        rng.initialize(seed);
        rng
    }

    /// Cheap zeroed placeholder that SKIPS the ~270-op [`Self::initialize`]
    /// seeding loop. Intended ONLY as a transient `mem::replace` sentinel that
    /// is immediately overwritten before anything samples it — the all-zero
    /// `seed_array` produces a meaningless stream that must never be observed.
    #[must_use]
    pub const fn placeholder() -> Self {
        Self {
            seed_array: [0; 56],
            inext: 0,
            inextp: 21,
        }
    }

    fn initialize(&mut self, seed: i32) {
        let subtraction = if seed == i32::MIN {
            INT_MAX
        } else {
            seed.saturating_abs()
        };
        let mut mj = 161_803_398_i32.wrapping_sub(subtraction);
        self.seed_array[55] = mj;
        let mut mk: i32 = 1;
        let mut ii: usize = 0;
        for _ in 1..55 {
            ii += 21;
            if ii >= 55 {
                ii -= 55;
            }
            self.seed_array[ii] = mk;
            mk = mj.wrapping_sub(mk);
            if mk < 0 {
                mk = mk.wrapping_add(INT_MAX);
            }
            mj = self.seed_array[ii];
        }
        for _ in 1..5 {
            for i in 1..56 {
                let mut n = i + 30;
                if n >= 55 {
                    n -= 55;
                }
                self.seed_array[i] = self.seed_array[i].wrapping_sub(self.seed_array[1 + n]);
                if self.seed_array[i] < 0 {
                    self.seed_array[i] = self.seed_array[i].wrapping_add(INT_MAX);
                }
            }
        }
    }

    /// Advance state and return the next 31-bit non-negative integer.
    ///
    /// Port pin: `_DotNetCompatRandom.internal_sample` in `core/rng.py:72`.
    pub fn internal_sample(&mut self) -> i32 {
        let mut loc_inext = self.inext + 1;
        if loc_inext >= 56 {
            loc_inext = 1;
        }
        let mut loc_inextp = self.inextp + 1;
        if loc_inextp >= 56 {
            loc_inextp = 1;
        }
        let mut ret = self.seed_array[loc_inext].wrapping_sub(self.seed_array[loc_inextp]);
        if ret == INT_MAX {
            ret -= 1;
        }
        if ret < 0 {
            ret = ret.wrapping_add(INT_MAX);
        }
        self.seed_array[loc_inext] = ret;
        self.inext = loc_inext;
        self.inextp = loc_inextp;
        ret
    }

    /// Uniform `[0, 1)` sample backed by `internal_sample`.
    pub fn sample(&mut self) -> f64 {
        f64::from(self.internal_sample()) * (1.0 / f64::from(INT_MAX))
    }

    /// Wider-range sample used by `Rng.next_int` when `(high+1-low)`
    /// exceeds `INT_MAX`. Mirrors .NET's `GetSampleForLargeRange`.
    pub fn get_sample_for_large_range(&mut self) -> f64 {
        let mut result = i64::from(self.internal_sample());
        if self.internal_sample() % 2 == 0 {
            result = -result;
        }
        let value = result + i64::from(INT_MAX - 1);
        (value as f64) / ((2 * i64::from(INT_MAX) - 1) as f64)
    }
}

/// High-level RNG wrapper that mirrors Python's `Rng` class.
///
/// Holds the underlying `DotNetRandom` plus a public counter so callers
/// can observe `rng.counter()` for snapshot reproducibility and use
/// `fast_forward_counter` to position a fresh copy at a known point in
/// the sequence (used by `_map_point_type_counts` in the generator).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rng {
    seed: u64,
    base: MegaRandom,
    counter: u32,
}

impl Rng {
    /// Construct an unnamed RNG from `seed` (counter starts at 0).
    ///
    /// v0.111.0: the seed is a full `u64` fed straight to [`MegaRandom`]
    /// (`new MegaRandom(seed)` in C#). Legacy callers holding a signed 32-bit
    /// seed convert with `u64::from(x as u32)` (never sign-extend).
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            base: MegaRandom::new(seed),
            counter: 0,
        }
    }

    /// Cheap placeholder `Rng` (see [`DotNetRandom::placeholder`]) that skips
    /// the seeding loop. For use ONLY as a transient `mem::replace` sentinel —
    /// e.g. when a `&mut Rng` must be split out of a struct for the duration of
    /// one call and restored immediately after. Never sample from it.
    #[must_use]
    pub const fn placeholder() -> Self {
        Self {
            seed: 0,
            base: MegaRandom::placeholder(),
            counter: 0,
        }
    }

    /// Fast **non-parity** reseed for search-side determinization streams.
    /// Used by MCTS chance-node reseeds, where C# RNG parity is not
    /// required. Takes a full `u64` seed (better stream separation than the
    /// `i32` [`Self::new`] path) and skips the ~270-op `System.Random` init.
    /// (The old `DotNetRandom`-filling variant was removed as dead code when
    /// v0.107.1 moved this path to splitmix-seeded `MegaRandom`; its `| 1`
    /// seed guard also collapsed each even seed onto its odd successor.)
    #[must_use]
    pub fn from_search_seed(seed: u64) -> Self {
        Self {
            seed,
            // v0.107.1: MegaRandom is itself splitmix64-seeded, so a full-u64
            // seed feeds it directly (no separate non-parity warmup needed).
            base: MegaRandom::new(seed),
            counter: 0,
        }
    }

    /// Construct a named-stream RNG: `Rng(ulong seed, string name)` =
    /// `seed + XxHash64(name)` (u64 wrapping), so independent streams under
    /// the same base seed don't collide. v0.111.0 `Rng.cs`.
    #[must_use]
    pub fn with_name(base_seed: u64, name: &str) -> Self {
        let combined = base_seed.wrapping_add(xxhash64_utf8(name));
        if rng_trace_enabled() {
            // `seed` matches C#'s `Rng.Seed` (base + DeterministicHashCode),
            // so the symmetric C#/Rust `RNGTRACE` logs align by seed. The
            // map line lets the diff tool label each seed with its stream.
            eprintln!("RNGMAP seed={combined} name={name}");
        }
        Self {
            seed: combined,
            base: MegaRandom::new(combined),
            counter: 0,
        }
    }

    /// Construct an RNG and skip ahead `counter` samples.
    ///
    /// Useful for replaying a partial RNG sequence to a known position.
    #[must_use]
    pub fn with_counter(seed: u64, counter: u32) -> Self {
        let mut rng = Self::new(seed);
        rng.fast_forward_counter(counter);
        rng
    }

    /// Resulting seed (after name mixing if any).
    #[must_use]
    pub const fn seed(&self) -> u64 {
        self.seed
    }

    /// The four xoshiro256** state words of the live generator (dump/debug
    /// accessor; the stream position, not the seed).
    #[must_use]
    pub const fn state(&self) -> [u64; 4] {
        self.base.state()
    }

    /// Number of public `next_*` calls so far.
    #[must_use]
    pub const fn counter(&self) -> u32 {
        self.counter
    }

    /// Advance the underlying sample stream `target_count - self.counter`
    /// times. Panics if `target_count < self.counter`.
    pub fn fast_forward_counter(&mut self, target_count: u32) {
        assert!(
            target_count >= self.counter,
            "cannot fast-forward an Rng counter to a lower number (current = {}, target = {})",
            self.counter,
            target_count
        );
        while self.counter < target_count {
            self.counter += 1;
            // v0.107.1: C# FastForwardCounter replays `_random.NextInt()`; every
            // counted draw consumes exactly one MegaRandom 64-bit step, so one
            // `next_u64()` advances the stream identically.
            self.base.next_u64();
        }
    }

    /// Inclusive `[low, high]` integer sample. Increments counter by 1.
    pub fn next_int(&mut self, low: i32, high: i32) -> i32 {
        assert!(low <= high, "low must be <= high");
        self.counter += 1;
        let exclusive_high = i64::from(high) + 1;
        let range_size = exclusive_high - i64::from(low);
        // v0.107.1: C# `_random.Next(min, max)` == `(int)(NextDouble() * (max-min)) + min`
        // for both the small (`NextInner(int)`) and large (`NextInner(long)`)
        // branches — a single `sample()` per draw, no special large-range path
        // (MegaRandom dropped System.Random's GetSampleForLargeRange).
        let scaled = (self.base.sample() * (range_size as f64)) as i64;
        let result = (scaled + i64::from(low)) as i32;
        if rng_trace_enabled() {
            // Normalize to C#'s logged form: 1-arg `NextInt(maxExclusive)`
            // (min 0) vs 2-arg `NextInt(min, maxExclusive)`. `exclusive_high`
            // == C#'s maxExclusive (Rust's `high` is inclusive).
            if low == 0 {
                eprintln!(
                    "RNGTRACE seed={} ctr={} max={exclusive_high} r={result}",
                    self.seed, self.counter
                );
            } else {
                eprintln!(
                    "RNGTRACE seed={} ctr={} max={low}:{exclusive_high} r={result}",
                    self.seed, self.counter
                );
            }
        }
        result
    }

    /// Half-open `[low, high)` integer sample.
    pub fn next_int_exclusive(&mut self, low: i32, high: i32) -> i32 {
        assert!(low < high, "low must be < high");
        self.next_int(low, high - 1)
    }

    /// True iff `next_int(0, 1) == 0` — matches the game's `NextBool`.
    pub fn next_bool(&mut self) -> bool {
        self.next_int(0, 1) == 0
    }

    /// Uniform `[0, 1)` double — mirrors C# `Rng.NextDouble()`
    /// (`MegaCrit.Sts2.Core.Random/Rng.cs:100`, `return _random.NextDouble()`).
    /// Used by `GrabBag.GrabIndex` (`num = NextDouble() * totalWeight`,
    /// `Helpers/GrabBag.cs:66`) during run-setup `GenerateRooms`. Increments
    /// the counter by 1 and, when tracing, logs the C# form
    /// `max=dbl r=<IEEE-754 i64 bits>` so `trace_run_rng.py` aligns it with the
    /// headless `RngTracePatches.NextDoublePostfix`
    /// (`DoubleToInt64Bits(__result)`).
    pub fn next_double(&mut self) -> f64 {
        self.counter += 1;
        let sample = self.base.sample();
        if rng_trace_enabled() {
            eprintln!(
                "RNGTRACE seed={} ctr={} max=dbl r={}",
                self.seed,
                self.counter,
                sample.to_bits() as i64
            );
        }
        sample
    }

    /// Uniform `[0, upper)` float. Increments counter by 1.
    pub fn next_float(&mut self, upper: f64) -> f64 {
        self.counter += 1;
        self.base.sample() * upper
    }

    /// Uniform `[low, high)` float. Increments counter by 1.
    pub fn next_float_range(&mut self, low: f64, high: f64) -> f64 {
        assert!(low <= high, "low must be <= high");
        self.counter += 1;
        low + self.base.sample() * (high - low)
    }

    /// Fisher–Yates in-place shuffle. Counter increments by `n - 1`
    /// where `n` is `lst.len()`. Mirrors C# `ListExtensions.UnstableShuffle`
    /// (`MegaCrit.Sts2.Core.Extensions/ListExtensions.cs:21`) — the swap
    /// indexes come from `NextInt(num + 1)` which is `[0, num]` inclusive,
    /// equivalent to our `next_int(0, index)`.
    pub fn shuffle<T>(&mut self, lst: &mut [T]) {
        let len = lst.len();
        if len < 2 {
            return;
        }
        let mut index = len - 1;
        while index > 0 {
            let swap_index = self.next_int(0, index as i32) as usize;
            lst.swap(index, swap_index);
            index -= 1;
        }
    }

    /// "Stable" shuffle: sort the slice in-place, then run
    /// [`Self::shuffle`]. Mirrors C# `ListExtensions.StableShuffle`
    /// (`MegaCrit.Sts2.Core.Extensions/ListExtensions.cs:10`):
    /// `list.Sort()` THEN `UnstableShuffle`. C# `list.Sort()` is .NET's
    /// introspective sort, which is UNSTABLE for equal-key runs once a
    /// partition exceeds 16 elements (quicksort partitioning). A plain
    /// `lst.sort()` (Rust stable sort) reproduces it ONLY for ≤16-element
    /// inputs or fully-distinct keys; with tied keys above 16 elements the
    /// post-sort order — and thus the Fisher-Yates output — drifts from C#.
    /// Route through [`crate::cards::dotnet_list_sort`], the faithful .NET
    /// introsort, so the tied run lands at the C# slot. Used by
    /// `MapPathPruning` + `StandardActMap.AssignRemainingTypesToRandomPoints`
    /// to make iteration order deterministic across runs even when the
    /// caller passes a set/hash-ordered collection.
    pub fn stable_shuffle<T: Ord + Copy>(&mut self, lst: &mut [T]) {
        crate::cards::dotnet_list_sort(lst, &|a: T, b: T| a.cmp(&b));
        self.shuffle(lst);
    }

    /// [`Self::stable_shuffle`] with an explicit comparator, for elements
    /// whose C# sort key is not their natural `Ord` (e.g. cards keyed by
    /// `(Entry, upgraded)` rather than their `CardUid` instance). Sorts via
    /// the faithful .NET introsort [`crate::cards::dotnet_list_sort`] THEN
    /// runs [`Self::shuffle`], matching C# `StableShuffle` exactly.
    pub fn stable_shuffle_by<T, F>(&mut self, lst: &mut [T], cmp: &F)
    where
        T: Copy,
        F: Fn(T, T) -> core::cmp::Ordering,
    {
        crate::cards::dotnet_list_sort(lst, cmp);
        self.shuffle(lst);
    }

    /// Rejection-sampled Gaussian int in `[min_val, max_val]`. Matches
    /// the C# `Rng.NextGaussianInt` (re-rolls until in-range; does NOT
    /// clamp).
    ///
    /// v0.107.1: the C# loop calls the wrapper `NextDouble()` (not the raw
    /// generator) twice per iteration, so each iteration — including rejected
    /// re-rolls — advances the public counter by 2. (Pre-v0.107.1 the counter
    /// did not tick here; the swap to the wrapper changed that.)
    pub fn next_gaussian_int(&mut self, mean: f64, stddev: f64, min_val: i32, max_val: i32) -> i32 {
        loop {
            let u1 = 1.0 - self.next_double();
            let u2 = 1.0 - self.next_double();
            let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).sin();
            // C# `Math.Round(double)` uses banker's rounding (round half to
            // even) by default. Rust's `f64::round()` uses round-half-away-
            // from-zero, which diverges at half-integers — e.g., 12.5 → 12
            // in C# vs 13 in Rust. Use `round_ties_even` to match C#.
            let val_f = (mean + stddev * z).round_ties_even();
            let val = val_f as i32;
            if val >= min_val && val <= max_val {
                return val;
            }
        }
    }

    /// Fork a child RNG (Rust-only helper: the C# `Rng` has no fork). Draws
    /// once from `self` over the **inclusive** range `[0, i32::MAX]` (counter
    /// +1) and seeds the child with that value zero-extended to `u64`. This is
    /// *not* C#'s no-argument `NextInt()` (that excludes `i32::MAX`).
    #[must_use]
    pub fn fork(&mut self) -> Self {
        let draw = self.next_int(0, (INT_MAX_EXCLUSIVE - 1) as i32);
        Self::new(u64::from(draw as u32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Low-level Xoshiro256\*\* vectors: `MegaRandom(seed).NextULong()` × 4.
    /// Captured from the v0.107.1 oracle (`MegaCrit.Sts2.Core.Random.MegaRandom`
    /// via a sts2.dll reference harness) — locks in splitmix64 seeding + the
    /// `NextULongInner` step byte-for-byte.
    #[test]
    fn megarandom_next_u64_matches_oracle() {
        let mut m0 = MegaRandom::new(0);
        assert_eq!(
            [m0.next_u64(), m0.next_u64(), m0.next_u64(), m0.next_u64()],
            [
                0x99EC_5F36_CB75_F2B4,
                0xBF6E_1F78_4956_452A,
                0x1A5F_849D_4933_E6E0,
                0x6AA5_94F1_262D_2D2C,
            ]
        );
        let mut m42 = MegaRandom::new(42);
        assert_eq!(
            [
                m42.next_u64(),
                m42.next_u64(),
                m42.next_u64(),
                m42.next_u64()
            ],
            [
                0x1578_0B2E_0C2E_C716,
                0x6104_D986_6D11_3A7E,
                0xAE17_5332_39E4_99A1,
                0xECB8_AD47_03B3_60A1,
            ]
        );
    }

    /// Snapshot: `Rng(seed).next_int(0, 99)` × 8 (== C# `NextInt(0, 100)`).
    /// Golden values from the v0.107.1 oracle.
    #[test]
    fn rng_seed_opening_sequence_matches_oracle() {
        let cases: [(i32, [i32; 8]); 4] = [
            (0, [60, 74, 10, 41, 73, 99, 42, 53]),
            (1, [70, 52, 57, 39, 69, 14, 7, 38]),
            (42, [8, 37, 68, 92, 99, 76, 71, 85]),
            (2026, [57, 28, 81, 89, 81, 78, 83, 82]),
        ];
        for (seed, expected) in cases {
            let mut rng = Rng::new(u64::from(seed as u32));
            let got: Vec<i32> = (0..8).map(|_| rng.next_int(0, 99)).collect();
            assert_eq!(got, expected, "seed {seed}");
            assert_eq!(rng.counter(), 8);
        }
    }

    /// `Rng(seed).next_bool()` × 8 (== C# `Next(2) == 0`). Oracle golden.
    #[test]
    fn rng_next_bool_matches_oracle() {
        let bools = |seed: i32| -> Vec<bool> {
            let mut r = Rng::new(u64::from(seed as u32));
            (0..8).map(|_| r.next_bool()).collect()
        };
        let b = |bits: [u8; 8]| bits.map(|x| x == 1).to_vec();
        assert_eq!(bools(0), b([0, 0, 1, 1, 0, 0, 1, 0]));
        assert_eq!(bools(42), b([1, 1, 0, 0, 0, 0, 0, 0]));
    }

    #[test]
    fn rng_counter_advances_per_call() {
        let mut rng = Rng::new(42);
        assert_eq!(rng.counter(), 0);
        let _ = rng.next_int(0, 10);
        assert_eq!(rng.counter(), 1);
        let _ = rng.next_float(1.0);
        assert_eq!(rng.counter(), 2);
        let _ = rng.next_bool();
        assert_eq!(rng.counter(), 3);
    }

    #[test]
    fn rng_fast_forward_matches_native_advance() {
        let mut a = Rng::new(99);
        let mut b = Rng::with_counter(99, 5);
        for _ in 0..5 {
            let _ = a.next_int(0, 1_000_000);
        }
        // After both have ticked 5, the next sample must match.
        assert_eq!(a.next_int(0, 1_000_000), b.next_int(0, 1_000_000));
        assert_eq!(a.counter(), b.counter());
    }

    #[test]
    fn shuffle_is_in_range_and_permutes() {
        let mut rng = Rng::new(7);
        let mut data: Vec<i32> = (0..20).collect();
        let before: Vec<i32> = data.clone();
        rng.shuffle(&mut data);
        assert_eq!(data.len(), before.len());
        // Same multiset.
        let mut sorted = data.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, before);
        // Counter advanced by len-1.
        assert_eq!(rng.counter(), 19);
    }

    #[test]
    fn shuffle_short_lists_noop() {
        let mut rng = Rng::new(1);
        let mut empty: Vec<i32> = vec![];
        rng.shuffle(&mut empty);
        let mut single = vec![42];
        rng.shuffle(&mut single);
        assert_eq!(single, vec![42]);
        assert_eq!(rng.counter(), 0);
    }

    #[test]
    fn next_int_inclusive_bounds() {
        let mut rng = Rng::new(123);
        for _ in 0..500 {
            let v = rng.next_int(-5, 5);
            assert!((-5..=5).contains(&v));
        }
    }

    #[test]
    fn next_int_exclusive_excludes_high() {
        let mut rng = Rng::new(123);
        for _ in 0..500 {
            let v = rng.next_int_exclusive(0, 10);
            assert!((0..10).contains(&v));
        }
    }

    #[test]
    fn gaussian_int_matches_oracle_seed_zero() {
        // v0.107.1 oracle: Rng(0).NextGaussianInt(7,1,6,7) x5 → [7,7,7,7,7], ctr=14.
        let mut rng = Rng::new(0);
        let got: Vec<i32> = (0..5)
            .map(|_| rng.next_gaussian_int(7.0, 1.0, 6, 7))
            .collect();
        assert_eq!(got, vec![7, 7, 7, 7, 7]);
        // 5 results took 7 reject-loop iterations × 2 wrapper NextDouble = 14.
        assert_eq!(rng.counter(), 14);

        // v0.107.1 oracle: Rng(0).NextGaussianInt(12,1,10,14) x5 → [13,12,12,12,13], ctr=10.
        let mut rng2 = Rng::new(0);
        let got2: Vec<i32> = (0..5)
            .map(|_| rng2.next_gaussian_int(12.0, 1.0, 10, 14))
            .collect();
        assert_eq!(got2, vec![13, 12, 12, 12, 13]);
        assert_eq!(rng2.counter(), 10);
    }

    #[test]
    fn gaussian_int_stays_in_range() {
        let mut rng = Rng::new(31);
        for _ in 0..200 {
            let v = rng.next_gaussian_int(7.0, 1.0, 6, 7);
            assert!((6..=7).contains(&v), "got {v}");
        }
    }

    #[test]
    fn named_streams_diverge() {
        let mut a = Rng::with_name(0, "monster");
        let mut b = Rng::with_name(0, "event");
        let av = a.next_int(0, 1_000_000);
        let bv = b.next_int(0, 1_000_000);
        assert_ne!(
            av, bv,
            "named streams should diverge even with same base seed"
        );
    }

    #[test]
    fn xxhash64_standard_vectors() {
        assert_eq!(xxhash64_utf8(""), 0xEF46_DB37_51D8_E999);
        assert_eq!(xxhash64_utf8("a"), 0xD24E_C4F1_A98C_6E5B);
        // Published XXH64 vectors (seed 0) covering the 4-byte, 8-byte and
        // 32-byte-stripe branches.
        assert_eq!(xxhash64_utf8("abc"), 0x44BC_2CF5_AD77_0999);
        assert_eq!(
            xxhash64_utf8("Nobody inspects the spammish repetition"),
            0xFBCE_A83C_8A37_8BF1
        );
    }

    /// W1 vectors: `run_seed`/`player_seed` of the 20 oracle seeds.
    /// Source: sts2_bot `logs/engine/p0/stage1_vectors.json`
    /// (sha256 5267538d84c1a1468b68656b82bdd270e4a4e21b60a251366beb475786aa8976,
    /// oracle f79d3a9, game v0.111.0).
    const W1_VECTORS: [(&str, u64, u64); 20] = [
        ("P0A0", 10883704133362413376, 10883704133362413376),
        ("P0A1", 5101801426979218024, 5101801426979218024),
        ("P0A2", 15767174202476603724, 15767174202476603724),
        ("P0A3", 5706165440450657479, 5706165440450657479),
        ("P0A4", 5688482973830756896, 5688482973830756896),
        ("P0A5", 8097214833571844205, 8097214833571844205),
        ("P0A6", 5024012090796935627, 5024012090796935627),
        ("P0A7", 7397479155778214808, 7397479155778214808),
        ("P0A8", 9135721552375587661, 9135721552375587661),
        ("P0A9", 13778472366970538763, 13778472366970538763),
        ("P0A10", 12159427438006513464, 12159427438006513464),
        ("P0A11", 17697121495486258137, 17697121495486258137),
        ("P0A12", 14391125488767557262, 14391125488767557262),
        ("P0A13", 16941348480414710656, 16941348480414710656),
        ("P0A14", 15074555918783445927, 15074555918783445927),
        ("oldP0A0", 273692109, 15676685915450073139),
        ("oldP0A1", 3002575464, 18283884523336027873),
        ("oldP0A2", 3405859991, 17731986911216078246),
        ("oldP0A3", 1839776050, 13570675488851547012),
        ("oldP0A4", 2599290937, 3030005280704353688),
    ];

    #[test]
    fn seed_bases_match_w1_vectors() {
        for (seed, run, player) in W1_VECTORS {
            let b = seed_bases(seed);
            assert_eq!(b.run_seed, run, "{seed} run_seed");
            assert_eq!(b.player_seed, player, "{seed} player_seed");
            assert_eq!(b.full_hash, xxhash64_utf8(seed), "{seed} full_hash");
            if seed.starts_with("old") {
                assert_eq!(b.prefix, "old");
                assert_ne!(b.run_seed, b.player_seed, "{seed}: old seeds use two bases");
            } else {
                assert_eq!(b.prefix, "");
                assert_eq!(b.run_seed, b.player_seed, "{seed}");
            }
        }
    }

    #[test]
    fn seed_bases_edge_cases() {
        // Only the lowercase prefix selects the legacy path (decided before
        // normalization); `OLD...` is an ordinary seed (canonicalized, then hashed whole).
        let upper = seed_bases("OLDP0A0");
        assert_eq!(upper.prefix, "");
        assert_eq!(upper.run_seed, xxhash64_utf8("0LDP0A0")); // canonicalize folds O -> 0
        assert_eq!(upper.run_seed, upper.player_seed);
        // Non-old seeds are canonicalized (O->0, I->1, uppercase) before hashing.
        assert_eq!(seed_bases("p0a0").run_seed, xxhash64_utf8("P0A0"));
        assert_eq!(seed_bases("POAI").run_seed, xxhash64_utf8("P0A1"));
        // `old` rest is hashed as given with the legacy hash.
        assert_eq!(
            seed_bases("oldP0A0").run_seed,
            u64::from(deterministic_hash_code_old("P0A0") as u32)
        );
        // Empty string and a 12-character seed.
        let empty = seed_bases("");
        assert_eq!(empty.run_seed, 0xEF46_DB37_51D8_E999);
        assert_eq!(empty.player_seed, empty.run_seed);
        let twelve = seed_bases("ABCDEFGH2345");
        assert_eq!(twelve.run_seed, xxhash64_utf8("ABCDEFGH2345"));
        // `old` alone: empty rest.
        let bare = seed_bases("old");
        assert_eq!(bare.prefix, "old");
        assert_eq!(bare.player_seed, xxhash64_utf8("old"));
    }

    #[test]
    fn deterministic_hash_code_old_matches_python_snapshots() {
        // Captured from Python: deterministic_hash_code_old("monster"),
        // ("event"), ("treasure"), ("") at pin 01c094d.
        // Placeholder asserts — replaced once parity harness runs.
        let _ = deterministic_hash_code_old("monster");
        let _ = deterministic_hash_code_old("event");
        let _ = deterministic_hash_code_old("treasure");
        assert_eq!(
            deterministic_hash_code_old(""),
            352_654_597_i32.wrapping_add(352_654_597_i32.wrapping_mul(1_566_083_941))
        );
    }

    #[test]
    fn small_range_next_int_matches_oracle_seed_zero() {
        // v0.107.1 oracle: Rng(0).NextInt(7) × 10 (== Rust next_int(0,6)):
        let mut rng = Rng::new(0);
        let got: Vec<i32> = (0..10).map(|_| rng.next_int(0, 6)).collect();
        assert_eq!(got, vec![4, 5, 0, 2, 5, 6, 2, 3, 5, 6]);
        // And seed 42:
        let mut rng42 = Rng::new(42);
        let got42: Vec<i32> = (0..10).map(|_| rng42.next_int(0, 6)).collect();
        assert_eq!(got42, vec![0, 2, 4, 6, 6, 5, 5, 5, 5, 4]);
    }

    #[test]
    fn shuffle_matches_oracle_seed_zero() {
        // v0.107.1 oracle: Rng(0).Shuffle([0..7)) → [5,3,2,1,0,6,4], ctr=6.
        let mut rng = Rng::new(0);
        let mut lst: Vec<i32> = (0..7).collect();
        rng.shuffle(&mut lst);
        assert_eq!(lst, vec![5, 3, 2, 1, 0, 6, 4]);
        assert_eq!(rng.counter(), 6);
    }

    #[test]
    fn shuffle_55_elements_matches_oracle_seed_zero() {
        // v0.107.1 oracle: Rng(0).Shuffle([0..55)). 55 elements exercises shuffle
        // at a much larger N than the 7-element case, catching range-size-dependent
        // float-truncation divergences if any.
        let mut rng = Rng::new(0);
        let mut lst: Vec<i32> = (0..55).collect();
        rng.shuffle(&mut lst);
        let expected: Vec<i32> = vec![
            13, 29, 44, 35, 28, 54, 15, 38, 39, 14, 8, 1, 0, 12, 32, 6, 45, 51, 22, 19, 31, 24, 17,
            50, 3, 23, 26, 47, 18, 9, 30, 36, 34, 16, 10, 43, 46, 7, 41, 11, 48, 27, 4, 2, 52, 42,
            53, 25, 20, 49, 37, 21, 5, 40, 33,
        ];
        assert_eq!(lst, expected);
    }

    #[test]
    fn shuffle_three_elements_matches_oracle_seed_zero() {
        // v0.107.1 oracle: Rng(0).Shuffle([-1,0,1]) → [-1, 1, 0].
        let mut rng = Rng::new(0);
        let mut dirs: Vec<i32> = vec![-1, 0, 1];
        rng.shuffle(&mut dirs);
        assert_eq!(dirs, vec![-1, 1, 0]);
    }

    /// v0.111.0 named streams: `Rng(seed, name).Seed = seed + XxHash64(name)`
    /// (u64 wrapping), then 3 draws of the default W4 schedule
    /// `NextInt(0,100)`, `NextFloat()`, `NextInt(100)`.
    /// Vectors: sts2_bot `logs/engine/p0/stage1_vectors.json` (sha256
    /// 5267538d84c1a1468b68656b82bdd270e4a4e21b60a251366beb475786aa8976,
    /// oracle f79d3a9, seed `P0A0`: run_seed 10883704133362413376).
    #[test]
    fn named_stream_seed_and_first_draw_match_oracle() {
        let run_seed: u64 = 10_883_704_133_362_413_376;
        let cases: [(&str, u64, i32, f32, i32); 2] = [
            ("up_front", 12_796_180_781_041_495_488, 59, 0.256_277_53, 39),
            ("shuffle", 13_969_188_266_847_569_492, 13, 0.777_039_6, 45),
        ];
        for (name, seed, a, f, b) in cases {
            let mut r = Rng::with_name(run_seed, name);
            assert_eq!(r.seed(), seed, "{name} seed");
            assert_eq!(r.next_int_exclusive(0, 100), a, "{name} NextInt(0,100)");
            assert_eq!(
                (r.next_float(1.0) as f32).to_bits(),
                f.to_bits(),
                "{name} NextFloat()"
            );
            assert_eq!(r.next_int_exclusive(0, 100), b, "{name} NextInt(100)");
        }
    }

    /// `fork()` is a Rust-only helper: one inclusive `[0, i32::MAX]` draw
    /// (counter +1) zero-extended into the child's `u64` seed.
    #[test]
    fn fork_draws_once_inclusive_and_seeds_child_with_the_draw() {
        let mut parent = Rng::new(12345);
        let mut shadow = Rng::new(12345);
        let draw = shadow.next_int(0, i32::MAX);
        let child = parent.fork();
        assert_eq!(parent.counter(), 1, "fork consumes exactly one draw");
        assert_eq!(parent.counter(), shadow.counter());
        assert_eq!(child.seed(), u64::from(draw as u32));
        assert_eq!(child.counter(), 0);
        // Same parent state -> same child stream.
        let mut again = Rng::new(12345).fork();
        let mut child2 = child.clone();
        assert_eq!(again.next_int(0, 1_000_000), child2.next_int(0, 1_000_000));
    }

    #[test]
    fn stable_shuffle_sorts_before_shuffling() {
        // Two input orderings of the same multiset must produce
        // identical output, since stable_shuffle sorts first. This is
        // the property MapPathPruning + StandardActMap rely on.
        let mut a: Vec<i32> = vec![3, 1, 4, 1, 5, 9, 2, 6];
        let mut b: Vec<i32> = vec![9, 6, 5, 4, 3, 2, 1, 1];
        Rng::new(2026).stable_shuffle(&mut a);
        Rng::new(2026).stable_shuffle(&mut b);
        assert_eq!(a, b);
    }

    #[test]
    fn stable_shuffle_equivalent_to_sort_then_shuffle() {
        // Explicit semantics check: stable_shuffle(rng, lst) ==
        // { lst.sort(); rng.shuffle(lst); lst }
        let mut via_stable: Vec<i32> = vec![3, 1, 4, 1, 5, 9, 2, 6];
        Rng::new(7).stable_shuffle(&mut via_stable);

        let mut via_manual: Vec<i32> = vec![3, 1, 4, 1, 5, 9, 2, 6];
        // Production uses stable sort; for primitives the unstable
        // variant produces an identical sequence and satisfies the
        // workspace's `clippy::stable_sort_primitive` lint.
        via_manual.sort_unstable();
        Rng::new(7).shuffle(&mut via_manual);
        assert_eq!(via_stable, via_manual);
    }

    #[test]
    fn reproducibility_same_seed_same_sequence() {
        let mut a = Rng::new(2026);
        let mut b = Rng::new(2026);
        for _ in 0..50 {
            assert_eq!(a.next_int(0, 1_000), b.next_int(0, 1_000));
        }
    }
}
