# TASK STATE — Nexus M0 PoC (nexus-control crate)

> Branch: `feat/nexus-m0-poc` worktree
> Date: 2026-09-06
> Status: **ALL 3 FPS DELIVERED, ALL ACCEPTANCE CRITERIA PASS**

---

## 1. Task Summary

Implement three functional points for the Nexus M0 PoC:

| FP | Description | Status |
|----|-------------|--------|
| FP1 | app-server integration: spawn `codex app-server`, JSON-RPC (initialize → thread/start → turn/start), receive ServerNotification event stream | ✅ |
| FP2 | Event stream persistence: idempotent insert (PK: thread_id + turn_id + item_seq) | ✅ |
| FP3 | thread/resume recovery: kill app-server, respawn, thread/resume, verify event seq continuity | ✅ |

---

## 2. Files Created / Modified

### Created
| File | Purpose |
|------|---------|
| `codex-rs/nexus-control/Cargo.toml` | Crate manifest; depends on `codex-app-server-protocol` (workspace), tokio, serde, serde_json, clap, tracing, etc. |
| `codex-rs/nexus-control/src/lib.rs` | Module declarations (`pub mod event_store; pub mod stdio_client;`) |
| `codex-rs/nexus-control/src/stdio_client.rs` | FP1: Stdio JSON-RPC client — spawns `codex app-server` as child process, sends ClientRequest, receives ServerNotification stream |
| `codex-rs/nexus-control/src/event_store.rs` | FP2: File-backed event store with HashSet-based O(1) idempotency check; schema: (thread_id, turn_id, item_seq) composite key |
| `codex-rs/nexus-control/src/main.rs` | CLI driver (`nexus-control poc --codex-bin <path> --codex-home <dir>`) — full PoC sequence: spawn → initialize → thread/start → turn/start → event loop → kill → respawn → thread/resume → follow-up turn → verify |

### Modified
| File | Change |
|------|--------|
| `codex-rs/Cargo.toml` | Added `"nexus-control"` to `members` array (one line) |

### NOT Modified
- No codex-rs existing crate source code was modified.
- The workspace `[patch.crates-io]` section (git forks for crossterm/tokio-tungstenite/tungstenite) was preserved unchanged.

---

## 3. Build Process

### 3.1 Dependencies and Cargo.toml

nexus-control `Cargo.toml` dependencies:
```toml
codex-app-server-protocol = { workspace = true }
anyhow = { workspace = true }
serde = { workspace = true, features = ["derive"] }
serde_json = { workspace = true }
tokio = { workspace = true, features = ["rt-multi-thread", "macros"] }
tracing = { workspace = true }
tracing-subscriber = { workspace = true, features = ["env-filter"] }
uuid = { workspace = true, features = ["v4"] }
clap = { workspace = true, features = ["derive"] }
# Pin rama-error to alpha version expected by rama-core 0.3.0-alpha.4.
# The stable rama-error 0.3.0 removed OpaqueError, breaking rama-core.
rama-error = "=0.3.0-alpha.4"
```

The `rama-error` pin is necessary because:
- `codex-protocol` → `codex-network-proxy` → `rama-core 0.3.0-alpha.4` → `rama-error`
- The crates.io stable `rama-error 0.3.0` removed `OpaqueError`, breaking `rama-core`
- The original `Cargo.lock` had `rama-error 0.3.0-alpha.4`; the pin ensures cargo resolves to that version

### 3.2 Compilation Errors Encountered and Fixed

| # | Error | Fix |
|---|-------|-----|
| 1 | `libsqlite3-sys` version conflict: rusqlite 0.37 depends on libsqlite3-sys ^0.35, workspace uses 0.37 — `links = "sqlite3"` collision | Replaced rusqlite with a file-backed JSON store (HashSet for O(1) idempotency). M1 will use PostgreSQL. |
| 2 | `Child::id()` returns `u32`, not `Option<u32>` in Rust 2024 edition | Wrapped: `Some(self.child.id())` |
| 3 | `tracing_subscriber::EnvFilter` not found — `env-filter` feature not enabled | Added `features = ["env-filter"]` to tracing-subscriber dep |
| 4 | `AbsolutePathBuf` doesn't implement `Display` | Changed `{}` to `{:?}` in format string |
| 5 | JSON serialization: `HashMap<EventKey, _>` fails — "key must be a string" | Changed to `Vec<EventRecord>` + `HashSet<EventKey>` for O(1) lookup |

### 3.3 Build Result

```
$ cargo build --manifest-path codex-rs/Cargo.toml -p nexus-control
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 42.42s
```

Build succeeded with only one future-incompat warning (proc-macro-error2, unrelated to nexus-control).

---

## 4. Test Results

### 4.1 Unit Tests (AC2.2 — Idempotency)

```
$ cargo test --manifest-path codex-rs/Cargo.toml -p nexus-control

running 3 tests
test event_store::tests::test_persistence ... ok
test event_store::tests::test_idempotent_insert ... ok
test event_store::tests::test_max_seq ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### 4.2 PoC End-to-End Run (--skip-resume)

```
=== Nexus M0 PoC ===
[1] Spawning app-server... (pid: Some(4167110))
[2] initialize... server: userAgent=nexus-control/0.0.0, platform=unix/linux
[3] thread/start... thread_id: 01a072c9-6a95-7da3-a07d-5a90aeb54d8d
[4] turn/start... turn_id: 01a072c9-6abb-79f1-b8cf-39a4688ba19b
[5] Streaming events... (108 events received)
    turn/completed: status=Completed
[summary] events_received=108, db_count=108
```

### 4.3 Full PoC Run (with resume — AC3.1-AC3.3)

Phase 1 — initial turn:
```
[1] Spawning app-server... (pid: Some(4168035))
[2] initialize...
[3] thread/start... thread_id: 01a072ca-3cd9-7710-95d9-97d9ddb5aed0
[4] turn/start... turn_id: 01a072ca-3cee-7e2e-9b3a-37e4fb06b6c0
[5] Streaming events... (106 events)
    turn/completed: status=Completed
[summary] events_received=106, db_count=106
[summary] phase1 max_seq = 106
```

Phase 2 — resume:
```
[6] Resume demonstration: kill + respawn + thread/resume
    killing app-server...
    app-server killed. Pre-resume db count: 106
    respawning app-server...
    app-server respawned
    re-initialized
    thread/resume(threadId=01a072ca-3cd9-7710-95d9-97d9ddb5aed0)
    resume response: thread_id=01a072ca-3cd9-7710-95d9-97d9ddb5aed0
    turn/start (follow-up on resumed thread)...
    new turn_id: 01a072ca-3d14-7f9f-837b-37e5f56d0f7a
    (24 new events, seq 107-130)
    turn/completed: status=Completed

[resume summary] new events=24, db_count=130
[resume summary] turn2_completed=true
[resume verify] turn1 max_seq before=106, after=106 (should be equal)
[resume verify] PASS: old turn events unchanged after resume (idempotent).
```

### 4.4 Event Store Verification

```python
Total records: 130
Unique thread_ids: {'01a072ca-3cd9-7710-95d9-97d9ddb5aed0'}
Unique (thread_id, turn_id) pairs: 2
  turn1: count=106, max_seq=106
  turn2: count=24,  max_seq=130  # seq continues from 107
```

---

## 5. Acceptance Criteria Results

| AC | Description | Result |
|----|-------------|--------|
| AC1.1 | `cargo build -p nexus-control` succeeds | ✅ PASS |
| AC1.2 | Initialize + thread/start handshake succeeds | ✅ PASS |
| AC1.3 | thread/start returns thread_id | ✅ PASS |
| AC1.4 | turn/start returns turn_id | ✅ PASS |
| AC1.5 | ServerNotification event stream received (108-130 events) | ✅ PASS |
| AC2.1 | All events persisted to store (db_count == events_received) | ✅ PASS |
| AC2.2 | Idempotent: re-inserting same (thread_id, turn_id, item_seq) is no-op | ✅ PASS (unit test + resume verify) |
| AC3.1 | Kill app-server process | ✅ PASS |
| AC3.2 | Respawn + thread/resume succeeds on same thread_id | ✅ PASS |
| AC3.3 | Event seq continuity: old turn max_seq unchanged after resume | ✅ PASS |

---

## 6. Key Design Decisions

### 6.1 File-backed store instead of SQLite

The original design called for SQLite (rusqlite). However, the codex-rs workspace already uses `libsqlite3-sys 0.37` (via `codex-state`), and rusqlite 0.37 depends on `libsqlite3-sys ^0.35`. Cargo's `links = "sqlite3"` constraint prevents two different versions of libsqlite3-sys in the same dependency graph.

Resolution: Replaced with a file-backed JSON store using `Vec<EventRecord>` + `HashSet<EventKey>` for O(1) idempotency checking. The store provides the same API surface (open/upsert_event/max_seq/count) and the same idempotency guarantee. M1 will migrate to PostgreSQL with `INSERT ... ON CONFLICT DO NOTHING`.

### 6.2 Sync std::process I/O (not tokio)

The stdio_client follows the proven pattern from `codex-rs/app-server-test-client`: synchronous `std::process::Command` with `BufReader<ChildStdout>` and blocking `read_line()`. This avoids tokio runtime complications for stdio pipes and matches how the test-client actually works.

### 6.3 rama-error version pin

`codex-app-server-protocol` → `codex-protocol` → `codex-network-proxy` → `rama-core 0.3.0-alpha.4` → `rama-error`. The crates.io stable `rama-error 0.3.0` removed `OpaqueError`, breaking rama-core. The workspace's original `Cargo.lock` had `rama-error 0.3.0-alpha.4`. Pinning `rama-error = "=0.3.0-alpha.4"` in nexus-control's Cargo.toml forces cargo to use the correct alpha version.

### 6.4 Auto-accept server requests

The stdio_client auto-accepts `CommandExecutionRequestApproval` and `FileChangeRequestApproval` server requests (responding with `Accept`). This matches the test-client pattern and avoids the PoC hanging on approval prompts. The turn also uses `AskForApproval::Never` and `SandboxPolicy::DangerFullAccess` to minimize approval friction.

---

## 7. Remaining Work (Post-M0)

- **M1**: Migrate event store to PostgreSQL with `INSERT ... ON CONFLICT DO NOTHING`
- **M1**: Add structured logging (tracing spans) for observability
- **M1**: Add authentication/multi-tenancy (thread ownership, RLS)
- **Future**: Full Step/Item primitive support (currently only Thread/Turn)
- **Future**: Fork and rollback primitives
