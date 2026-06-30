# Config Path Consistency And Reload Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `settings.exe` and `im_engine.dll` resolve the same `config.toml`, resolve relative table paths from the config file directory, and hot-reload config/table changes safely.

**Architecture:** Extract config-path and relative-path resolution into `core_engine` as a platform-agnostic helper. Refactor `im_engine` to hold a hot-swappable runtime snapshot and reload it from a directory watcher without tearing down active services.

**Tech Stack:** Rust workspace crates, `core_engine`, `im_engine`, `settings`, `arc-swap`, `notify`, `cargo test`

---

### Task 1: Shared Config Path Rules

**Files:**
- Create: `core_engine/src/config_path.rs`
- Modify: `core_engine/src/lib.rs`
- Modify: `core_engine/src/config.rs`

- [ ] Add shared helpers for resolving portable-vs-AppData config locations and config-relative resource paths.
- [ ] Cover portable mode, AppData fallback, and relative path resolution with unit tests.

### Task 2: Reuse Shared Rules In Settings

**Files:**
- Modify: `windows/settings/src/config_path.rs`
- Modify: `windows/settings/src/state.rs`

- [ ] Replace duplicated path logic with `core_engine` shared helpers.
- [ ] Ensure table directory state uses config-relative resolved paths.

### Task 3: Hot-Swappable im_engine Runtime

**Files:**
- Modify: `windows/im_engine/src/lib.rs`
- Modify: `windows/im_engine/src/factory.rs`

- [ ] Replace the static immutable engine snapshot with an `ArcSwap`-backed runtime snapshot.
- [ ] Load config and dictionary through the shared config-path helpers and config-relative path resolution.
- [ ] Add focused tests around path resolution and snapshot reload decisions where possible.

### Task 4: File Watcher Reload Loop

**Files:**
- Modify: `windows/im_engine/Cargo.toml`
- Create: `windows/im_engine/src/reload.rs`
- Modify: `windows/im_engine/src/lib.rs`

- [ ] Watch the config directory for `config.toml` and active table changes.
- [ ] On successful reload, atomically publish the new runtime snapshot.
- [ ] On reload failure, keep the old snapshot and log the error.

### Task 5: Verification

**Files:**
- Modify: tests touched above as needed

- [ ] Run targeted crate tests for `core_engine`, `settings`, and `im_engine`.
- [ ] Run a workspace check or tests to confirm the new dependencies and signatures compile together.
