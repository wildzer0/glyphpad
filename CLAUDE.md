# GlyphPad: istruzioni per Claude

La specifica vincolante del progetto è importata qui sotto. Se qualcosa è ambiguo, contraddittorio o tecnicamente sbagliato, chiedi invece di decidere da solo.

@docs/SPEC.md

## Modo di lavorare

- **Una milestone alla volta** (§14). Su indicazione dell'utente, che prevale sul §15, **non serve proporre un piano prima di ogni milestone**: si passa direttamente all'implementazione. Le domande su punti ambigui o contraddittori restano necessarie. Alla fine: test verdi, clippy pulito, `CHANGELOG.md` aggiornato e riepilogo.
- **Git:**
  - solo il branch `main`: mai branch, worktree o PR;
  - mai push forzati né riscrittura della storia pubblicata;
  - commit piccoli e atomici in formato Conventional Commits (`feat:`, `fix:`, `refactor:`, `test:`, `ci:`, `docs:`, `chore:`, con scope opzionale come `feat(platform):`);
  - **conferma esplicita prima di ogni `git push` e prima di creare o pushare un tag**;
  - mai committare segreti, chiavi, config o catture personali.
- **CHANGELOG:** aggiorna la sezione `## [Unreleased]` (Keep a Changelog) a ogni modifica rilevante per l'utente.
- **Prove sull'hardware:** il touchpad PTP è sulla macchina dell'utente. Quando un passo lo richiede, fermati e indica il comando esatto da lanciare e cosa deve rimandarti.
- **Ambiente:** lo sviluppo avviene in WSL2 (Ubuntu) su una macchina Windows 11; la toolchain Windows è raggiungibile come `cargo.exe`.

## Convenzioni del codice

- Rust stable, edizione 2024. `unsafe` vietato a livello di workspace (`unsafe_code = "deny"`): solo i moduli FFI di piattaforma lo abilitano con `#[allow(unsafe_code)]`, commentando il *perché* di ogni blocco.
- `glyphpad-core` non dipende da API dell'OS.
- **Nessun supporto macOS**, nemmeno `cfg(target_os = "macos")` predisposti.
- Tutte le soglie spaziali sono in **millimetri**. Le soglie del recognizer hanno un commento che ne spiega la scelta.
- Il repository GitHub è nominato **solo** in `glyphpad_updater::GITHUB_REPO`, oltre che nel metadato `repository` di `Cargo.toml`.
- Errori: `thiserror` nelle librerie, `anyhow` nel binario e in xtask.
- Non inventare API di sistema: verifica firme e comportamenti nella documentazione o con un piccolo test.
- Ogni dipendenza non ovvia va giustificata (nel piano o in un commento in `Cargo.toml`), preferendo crate maturi.
- Commenti e messaggi del codice in inglese.

## Struttura

```
Cargo.toml            # workspace; [workspace.package] version è l'unica fonte della versione
.cargo/config.toml    # alias `cargo xtask`, +crt-static per Windows MSVC
crates/core           # glyphpad-core: modello dati, gesture engine, recognizer, config
crates/platform       # glyphpad-platform: trait + backend Windows/Linux (cfg-gated)
crates/actions        # glyphpad-actions: esecuzione azioni
crates/gui            # glyphpad-gui: finestra egui/eframe (backend glow)
crates/updater        # glyphpad-updater: aggiornamento da GitHub Releases
crates/app            # glyphpad: binario (CLI, tray, loop principale)
xtask                 # automazioni: release, changelog, check-tag
tests/fixtures        # catture touch reali (.jsonl) per i test di regressione
docs/                 # SPEC.md, ARCHITECTURE.md (architettura e limiti noti)
.github/workflows     # ci.yml (push su main), release.yml (tag v*.*.*)
```

## Comandi utili

```sh
# Verifica completa (la stessa della CI)
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release

# Stessa verifica sulla toolchain Windows MSVC, da WSL
# (target dir su un disco Windows: build sul percorso UNC più veloci e affidabili)
WSLENV=CARGO_TARGET_DIR/p CARGO_TARGET_DIR=/mnt/c/Users/temporary/glyphpad-target \
  cargo.exe test --workspace

# Release: check, bump della versione, changelog, commit e tag annotato (senza push)
cargo xtask release 0.2.0 --dry-run
cargo xtask release 0.2.0
cargo xtask changelog 0.2.0     # note di rilascio di una versione
cargo xtask check-tag v0.2.0    # coerenza tra tag e versione (usato in CI)

# Pipeline
gh run watch
gh release view v0.2.0
```
