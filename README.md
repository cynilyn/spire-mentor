# sts2-sim

[![CI](https://github.com/tfriedel/sts2-sim/actions/workflows/ci.yml/badge.svg)](https://github.com/tfriedel/sts2-sim/actions/workflows/ci.yml)

A research stack around **Slay the Spire 2** combat: a from-scratch Rust
engine that replays the game's mechanics bit-for-bit, MCTS and run-level AI
on top of it, a Python harness that proves the engine against the real game
code, PPO-trained run policies, and a browser "workbench" for studying fights
and runs.

> Slay the Spire 2 is (c) Mega Crit; this project is not affiliated with or
> endorsed by them. The repository carries the game's **text and extracted
> data** (so everything works out of the box) but **no art, audio, game
> assemblies or decompiled code** — the tooling extracts those from **a copy
> of the game you own** ([docs/bring-your-own-game.md](docs/bring-your-own-game.md))
> into git-ignored paths. See [NOTICE](NOTICE) and [PROVENANCE.md](PROVENANCE.md).

## Layout

| Subproject | Language | What it is |
|---|---|---|
| [`sts2-rs/`](sts2-rs/) | Rust | The combat/run engine, MCTS, run-AI, CLI, PyO3 bindings, and the browser workbench's engine layer (wasm). Builds and tests with nothing else present. |
| [`sts2-pycli/`](sts2-pycli/) | Python + C# | The integration hub: a headless C# harness that drives the real game assembly (your copy) as a parity oracle, the C#-vs-Rust parity suite, PPO training, the workbench web app, the real-game advisor client. |
| [`sts2-wiki/`](sts2-wiki/) | Python / Astro | Extraction tooling: decompiles your game copy, extracts data, art and sound, and can generate a static wiki from them. |
| `scripts/import_game.py` | Python | `just import-game` — the one command that produces every game-derived input from your install. |

How the pieces fit: [ARCHITECTURE.md](ARCHITECTURE.md).

## Quickstart

Three tiers; each needs more than the last. Prerequisites and details:
[docs/bring-your-own-game.md](docs/bring-your-own-game.md).

**Engine only — no game needed.** Rust 1.96 (pinned by
`sts2-rs/rust-toolchain.toml`), `cargo-nextest`, `sccache` (or
`RUSTC_WRAPPER=` to bypass), `just`.

```sh
just rs-check      # cargo check --workspace --all-targets
just rs-test       # cargo nextest run --workspace  (never bare `cargo test`)
just rs-doc-test   # cargo test --workspace --doc   (nextest cannot run doctests)
```

**Oracle — the C# parity harness.** Add the game, the .NET 9 SDK, `uv`.

```sh
just import-game /path/to/Slay\ the\ Spire\ 2 --tier oracle
cd sts2-pycli && uv sync --dev && just build-cs && uv run pytest tests
```

**Workbench with real art (and the decompiled source).** Add `ilspycmd`, Node.

```sh
just import-game /path/to/Slay\ the\ Spire\ 2          # tier = workbench
cd sts2-pycli && just wasm-build && just workbench-static   # web/dist
just workbench                                          # or: the Python-backed server on :8077
```

Without the game the workbench still builds and runs with the real card
text; every image is a neutral placeholder tile and there is no sound until
`just import-game` extracts the art (and, with the FMOD SDK, the audio).

## Testing

- Rust: `cargo nextest run --workspace` **and** `cargo test --workspace --doc`
  from `sts2-rs/`. Bare `cargo test` shares one process across tests and
  produces order-dependent false failures here. Game-dependent tests skip
  when the game data is absent.
- Python: `uv run pytest tests` from `sts2-pycli/`. Tests that need the C#
  oracle carry the `oracle` marker and skip without `lib/sts2.dll`;
  `-m "not oracle"` is the game-free subset CI runs.
- Web: `npm test` from `sts2-pycli/web/` (vitest); suites that need the
  extracted audio/art skip without them.

## Contributing

[CONTRIBUTING.md](CONTRIBUTING.md) — including how this repository relates
to the private development repo it is synced from.

## License

MIT — [LICENSE](LICENSE); `sts2-rs/`, `sts2-pycli/` and `sts2-wiki/` carry
their own MIT files. The grant covers the original work only; nothing from
the game is licensed or included.
