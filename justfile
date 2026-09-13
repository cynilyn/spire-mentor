# sts2-sim umbrella tasks. Most work runs inside the subprojects
# (`just --list` in sts2-rs/, sts2-pycli/, sts2-wiki/).

# Rust: fast typecheck of the whole workspace
rs-check:
    cd sts2-rs && cargo check --workspace --all-targets

# Rust: canonical test runner (nextest — never bare `cargo test`)
rs-test:
    cd sts2-rs && cargo nextest run --workspace

# Rust: doctests — the one `cargo test` invocation (nextest cannot run them)
rs-doc-test:
    cd sts2-rs && cargo test --workspace --doc

# Python: the test suite (tests that need the C# oracle skip without the game)
py-test:
    cd sts2-pycli && uv run pytest tests

# Web: vitest (suites that need the extracted art/audio skip without them)
web-test:
    cd sts2-pycli/web && npm test

# Everything CI runs, in one go
check: rs-check rs-test rs-doc-test py-test web-test

# Regenerate every game-derived input from YOUR game install
# (docs/bring-your-own-game.md). `just import-game --status` shows what exists.
import-game *args:
    python3 scripts/import_game.py {{args}}

# Python envs for the two Python subprojects
setup:
    cd sts2-pycli && uv sync --dev
    cd sts2-wiki && uv sync
    rustup component add rust-analyzer clippy rustfmt 2>/dev/null || true
