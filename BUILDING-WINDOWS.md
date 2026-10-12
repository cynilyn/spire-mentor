# Building on Windows

How to compile and test **spire-mentor** on native Windows (no WSL). The Rust
combat engine (`sts2-rs/`) builds and tests cleanly this way. The full Python
+ C# + workbench stack (`sts2-pycli/`) is smoother under WSL2 — see
[Caveats](#caveats).

## 1. Install the toolchain

Run the idempotent bootstrap script from the repo root (safe to re-run — it
skips anything already present):

```powershell
.\install-windows-deps.ps1            # full stack
.\install-windows-deps.ps1 -RustOnly  # just the Rust engine deps
```

If PowerShell blocks it:
`powershell -ExecutionPolicy Bypass -File .\install-windows-deps.ps1`.

What each tier needs:

| Tier | Tools | Installed via |
|------|-------|---------------|
| **Rust engine** (`sts2-rs`) | `just`, `cargo-nextest`, rust-analyzer | winget / cargo / rustup |
| **Python CLI** (`sts2-pycli`) | Python 3.13 | `uv python install` |
| **C# runner** (`sts2-pycli/src`) | .NET 9 SDK | winget |
| **Web UIs** | Node.js LTS | winget |

Prerequisites the script verifies but does **not** install: `rustup`/`cargo`,
`uv`, `git`, `git-lfs`, and **VS Build Tools 2022 with the "Desktop
development with C++" workload** (provides `link.exe` — Rust cannot link
without it).

## 2. Build & test the Rust engine

```powershell
cd sts2-rs
cargo build --workspace
cargo nextest run --workspace   # canonical test runner — NOT `cargo test`
```

Expected: full workspace builds, **6092 tests pass**.

> Use `cargo nextest`, never `cargo test` — nextest runs one process per test,
> isolating the process-global effect/relic registry. Plain `cargo test` shares
> one process and produces order-dependent false failures.

## 3. The one Windows gotcha: the `sts2-py` PyO3 crate needs a real Python

The `sts2-py` crate embeds a Python interpreter and needs a **real** one at
**both build and run time**. Two traps on Windows:

1. The Microsoft Store `python.exe` alias in `%LOCALAPPDATA%\Microsoft\WindowsApps`
   is a dead stub that hijacks auto-detection — PyO3 finds it and fails with
   *"no Python 3.x interpreter found"*.
2. The compiled test binary must find `pythonNNN.dll` at load time, or it aborts
   with `0xc0000135` (STATUS_DLL_NOT_FOUND).

**Fix once (machine-local, per-user — never commit this):** point PyO3 straight
at a real interpreter and put its folder on PATH. Using a uv-managed Python:

```powershell
uv python install 3.13
$py = "$env:APPDATA\uv\python\cpython-3.13-windows-x86_64-none"

# Persist for your user account (survives new terminals):
[Environment]::SetEnvironmentVariable('PYO3_PYTHON', "$py\python.exe", 'User')
$userPath = [Environment]::GetEnvironmentVariable('Path','User')
if ($userPath -notlike "*$py*") {
    [Environment]::SetEnvironmentVariable('Path', "$py;$userPath", 'User')
}
```

Open a **new terminal** afterward. Then `cargo nextest run --workspace` works
with no per-session setup.

- `PYO3_PYTHON` (not PATH order) is what defeats the Store stub — it's
  unambiguous.
- These are user-env settings, **not** `sts2-rs/.cargo/config.toml`: that file
  is committed and shared, and Cargo config cannot scope an env var by OS, so a
  Windows path there would break every Linux/macOS dev.
- If uv later drops 3.13 for a newer minor, the folder name changes
  (`cpython-3.14-…`) and you repoint `PYO3_PYTHON` once. Within 3.13.x it's
  stable.
- `cargo build --workspace --exclude sts2-py` needs none of this.

## Line endings

The census/parity golden files under `sts2-rs/tests/parity/` are compared
byte-for-byte against `\n`-built strings. `sts2-rs/.gitattributes` pins them
(and `*.rs`) to `eol=lf` so `core.autocrlf=true` doesn't check them out as CRLF
and fail every golden comparison. Leave that rule in place; if goldens still
show CRLF locally, re-checkout them or strip `\r`.

## Caveats

- The root `justfile` runs under PowerShell on Windows (`rs-check`,
  `rs-test`, `rs-doc-test`, `py-test`, `web-test`). The subproject justfiles
  and `setup.sh` / `scripts/wheel-venv.sh` are written for Unix shells (bash
  shebangs, `.venv/bin/activate`, `fuser`, macOS/Linux Steam paths); they need
  an `sh` on PATH (Git Bash) and recipes like `workbench`, `sim-ui`,
  `build-rust`, and `setup` will still break on those Unix-isms.
- `sccache` is not required; set `$env:RUSTC_WRAPPER="sccache"` to use it.
- The C# headless runner also needs the game DLLs — `just import-game`
  (docs/bring-your-own-game.md) stages them from your install.
- For the full Python + C# + workbench stack, **WSL2 (Ubuntu) is the path of
  least resistance.** The Rust engine alone is happy on native Windows.
