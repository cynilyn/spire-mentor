"""Seed canonicalization shared by both backends.

The C# engine's :class:`RunRngSet` hashes the seed string as-is, while the
Rust port's ``RunManager(string_seed=…)`` canonicalizes first (uppercase +
``O`` → ``0`` + ``I`` → ``1``) before hashing. For non-canonical user input
the two engines would otherwise compute different ``Seed`` ints and diverge
from step zero — same input, totally different runs.

The workbench applies :func:`canonicalize_seed` once at session start so
both backends see the same canonical string. Idempotent: a string that is
already canonical (e.g. a game-generated 10-char alphanumeric like
``"Y3X2YZSBQJ"``) is returned unchanged.
"""

from __future__ import annotations


def canonicalize_seed(seed: str) -> str:
    """Normalize a user-entered seed to the form the game would generate.

    Matches ``sts2_core::canonicalize_seed`` in the Rust port: trim,
    uppercase, and fold the visually ambiguous letters ``O`` → ``0`` and
    ``I`` → ``1``.
    """
    return seed.strip().upper().replace("O", "0").replace("I", "1")
