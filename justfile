# spire-mentor umbrella tasks. Most work runs inside the subprojects
# (`just --list` in sts2-rs/, sts2-pycli/, sts2-wiki/).
#
# Windows: the recipes in THIS file run under PowerShell (each body is one
# plain command, no `&&`), so `just rs-check` works from a PowerShell prompt.
# The subproject justfiles are bash — run those (and `workbench-local`, which
# calls into sts2-pycli/) from Git Bash. Needs just >= 1.38.

set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

python := if os_family() == "windows" { "python" } else { "python3" }

# Rust: fast typecheck of the whole workspace
[working-directory: 'sts2-rs']
rs-check:
    cargo check --workspace --all-targets

# Rust: canonical test runner (nextest — never bare `cargo test`)
[working-directory: 'sts2-rs']
rs-test:
    cargo nextest run --workspace

# Rust: doctests — the one `cargo test` invocation (nextest cannot run them)
[working-directory: 'sts2-rs']
rs-doc-test:
    cargo test --workspace --doc

# Python: the test suite (tests that need the C# oracle skip without the game)
[working-directory: 'sts2-pycli']
py-test:
    uv run pytest tests

# Web: vitest (suites that need the extracted art/audio skip without them)
[working-directory: 'sts2-pycli/web']
web-test:
    npm test

# Everything CI runs, in one go
check: rs-check rs-test rs-doc-test py-test web-test

# docs/bring-your-own-game.md; `just import-game --status` shows what exists.
# Regenerate every game-derived input from YOUR game install
import-game *args:
    {{python}} scripts/import_game.py {{args}}

# Run `just import-game` first for real art and sound. Please don't host the
# build publicly: the art and audio are the game's, extracted from your copy
# for your own use. Windows: run from Git Bash.
# Build the browser workbench and serve it on http://127.0.0.1:4173 (this machine only)
[working-directory: 'sts2-pycli']
workbench-local:
    just workbench-static
    just workbench-static-preview

# Python envs for the two Python subprojects
setup:
    uv --directory sts2-pycli sync --dev
    uv --directory sts2-wiki sync
    -rustup component add rust-analyzer clippy rustfmt
