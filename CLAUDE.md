# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this repository is

Nexus is an enterprise Agent-Native platform built **on top of** OpenAI's open-source Codex CLI (the "Harness"). The repo is a Cargo workspace plus a Bazel overlay; the actual Codex execution kernel lives in `codex-rs/` (111+ crates, Apache-2.0, Rust 2024 edition, Tokio async runtime). `codex-main/` is a reference/upstream snapshot of the same Codex tree — do your edits in `codex-rs/`, not `codex-main/`.

The strategic framing lives in `docs/architecture/Nexus 基于CodexHarness的企业级Agent平台_系统设计与实施路线图.md`. Its central thesis, which should guide any non-trivial change:

> Codex Harness is a single-user, local, interruptible Agent engine. Nexus turns it into a multi-tenant, metered, auditable, crash-safe enterprise service. The two must stay strictly separated, and the kernel must not be modified.

Three rules follow from that:
- **Integrate via `app-server` (JSON-RPC), never `codex exec` or the in-process SDK**, for anything that needs long-lived sessions, approvals, or crash recovery. `codex exec` is one-shot; the SDK is in-process. Only app-server gives Threads, bidirectional event streams, `turn/interrupt`, protocol-level approvals, and `thread/resume`.
- **Don't modify the Harness kernel.** Upstream moves daily. Express tenant/role differences as generated `config.toml` + `execpolicy` rulesets, event bridging, and outer wrappers. Patches that are unavoidable go in `patches/` with an upstream-tracking board.
- **Session truth lives in the cloud control plane, not the Harness.** Codex persists locally (SQLite `state`/`thread-store` + rollout files). The control plane consumes app-server events → Postgres, syncs rollout files to object storage, so a Pod dying never loses a session.

## Build, test, lint, format

All Rust commands go through `just` (which `cd`s into `codex-rs/` itself). Install `just`, `rg`, and `cargo-insta` if missing.

```shell
just fmt                  # format justfile, Rust, Bazel/Starlark, Python SDK + scripts (run after any code change, no approval needed)
just fmt-check            # check formatting without modifying
just fix -p <crate>       # cargo clippy --fix scoped to one crate (preferred over workspace-wide)
just clippy -p <crate>    # clippy check only
just test -p <crate>      # run one crate's tests (do NOT use `cargo test` directly)
just test                 # full suite — ask before running; avoid --all-features (bloats target/)
just bench                # cargo benchmarks (divan); `just bench-smoke` for a 1-iteration dry run
just codex                # cargo run --bin codex (the CLI)
just app-server-test-client
just write-config-schema           # regenerate codex-rs/core/config.schema.json after changing ConfigToml/nested config types
just write-app-server-schema       # regenerate app-server protocol schema fixtures after API shape changes (add --experimental as needed)
just write-hooks-schema            # regenerate hooks schema fixtures
just bazel-lock-update             # after Cargo.toml/Cargo.lock changes — refresh MODULE.bazel.lock (CI verifies drift)
just bazel-codex                   # build+run codex via Bazel
just bazel-test / bazel-clippy / bazel-argument-comment-lint
just argument-comment-lint         # the /*param_name*/ opaque-literal-arg lint (Bazel-backed, slow first run)
just log -- <args>                 # tail logs from the state SQLite DB
```

Run a single test with nextest: `just test -p <crate> -- <test_name>` (the recipe forwards positional args). Be patient with Rust commands — never kill them by PID; the build lock makes them slow.

Other tooling: `ruff` (excludes `sdk/python`, `codex-rs/vendor`), `prettier` (run `pnpm format`/`format:fix` at repo root for JSON/MD/YAML/JS), `codespell` (config in `.codespellrc`), `markdownlint-cli2`. Bazel is pinned to 9.0.0 (`.bazelversion`).

## Architecture: the Codex Harness (L5)

The agent loop lives in `codex-rs/core` (`codex-core`). One **Turn** runs seven phases: admission → snapshot → sampling → tool dispatch → result writeback → compaction decision → complete/interrupt. Key primitives, which you must keep distinct or you'll design the data model wrong:

- **Thread** — app-server's long-lived session object (`fork`/`resume`/`archive`/rollback). *Protocol layer.*
- **Session** — Core's internal runtime holder of active state, input queue, history. **Not exposed externally.**
- **Turn** — one task round-trip (user msg → model reasoning → tool calls → results), possibly multiple samples.
- **Step** — one sampling snapshot within a Turn.
- **Item** — a persistable atomic fact in a Turn (user message, reasoning, shell command, file edit, tool result).

Other load-bearing crates (reuse these, don't rewrite):
- `execpolicy` — rule-based command allow/deny engine (parser + evaluator), decoupled from the session layer. **This is the natural policy-injection carrier** for per-tenant command rules.
- `sandboxing` / `linux-sandbox` / `bwrap` / `windows-sandbox-rs` — OS-level command isolation (Seatbelt / Landlock+seccomp+bubblewrap / Windows restricted token). These are **single-tenant command-level isolation**, not multi-tenant isolation — tenant isolation is the control plane's job.
- `codex-mcp` — MCP client; use `codex-rs/codex-mcp/src/mcp_connection_manager.rs` for tool/tool-call mutations, keep changes minimal.
- `app-server` + `app-server-protocol` — JSON-RPC 2.0 over stdio / unix socket / WebSocket. **Main integration surface.** Thread→Turn→Item primitives; `generate-ts` / `generate-json-schema` emit types.
- `state` (SQLite) / `thread-store` / `rollout` — local persistence; the control plane mirrors these to cloud storage.
- `model-provider-info` / `responses-api-proxy` / `ollama` / `lmstudio` — model abstraction; point at the self-built Model Gateway.
- `skills` / `hooks` — Markdown skills + lifecycle hooks; foundation for the enterprise skill market.
- `collaboration-mode-templates` — sub-agent / multi-agent orchestration reference.
- `cli` — the `codex` binary (`src/main.rs`) and `logs_client`.

## Hard rules inherited from AGENTS.md (upstream Codex conventions)

These are enforced by CI and reviewers; violating them produces noisy diffs and rejected PRs.

- **Resist adding code to `codex-core`.** It's already bloated. New concepts go in an existing smaller crate or a brand-new workspace crate — refactor to make that happen. Push back on PRs that bloat `core`.
- **Never add or modify code referencing `CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR` or `CODEX_SANDBOX_ENV_VAR`.** You run inside a sandbox where `CODEX_SANDBOX_NETWORK_DISABLED=1` is set on `shell`-tool calls and `CODEX_SANDBOX=seatbelt` on seatbelt children; existing code uses these to early-exit tests you can't run. Don't break that.
- **Bazel doesn't auto-expose source files to Rust build-time reads.** If you add `include_str!`, `include_bytes!`, `sqlx::migrate!`, etc., update the crate's `BUILD.bazel` (`compile_data` / `build_script_data` / test data) or Bazel fails even when Cargo passes.
- **Cargo dep changes require `just bazel-lock-update`**, included in the same change. CI verifies `MODULE.bazel.lock` drift.
- **Module size:** target Rust modules under 500 LoC (excluding tests); above ~800 LoC, put new functionality in a new module. This applies especially to high-touch files: `tui/src/app.rs`, `tui/src/bottom_pane/chat_composer.rs`, `tui/src/bottom_pane/footer.rs`, `tui/src/chatwidget.rs`, `tui/src/bottom_pane/mod.rs`. Don't add standalone methods to `chatwidget.rs` unless trivial — keep it focused on orchestration.
- **Don't create small helper methods referenced only once.**
- **Clippy style (enforced):** collapse nested `if` ([collapsible_if](https://rust-lang.github.io/rust-clippy/master/index.html#collapsible_if)); inline `format!` args; method refs over closures; avoid bool/ambiguous-`Option` params that force `foo(false)`/`bar(None)` callsites — prefer enums/named methods/newtypes.
- **Opaque positional literals** (`None`, bools, numbers) take an exact `/*param_name*/` comment (the `argument-comment-lint`); exempt when sole non-self arg matches the method name (`.enabled(false)`). Don't add these for string/char literals.
- **Traits:** include doc comments. Discourage `#[async_trait]` and `#[allow(async_fn_in_trait)]`; prefer native RPITIT `fn foo(&self, ...) -> impl Future<Output = T> + Send;`.
- **Tracing:** instrument the function definition with `#[tracing::instrument(...)]`, not `.instrument(...)` at callsites; check the callee isn't already instrumented first.
- **Make `match` exhaustive; avoid wildcard arms.** Prefer private modules with explicit public crate API.
- **Change size:** ≤800 changed lines for non-mechanical changes, ≤500 for complex logic; split larger changes into reviewable stages.

## app-server protocol work (v2 only)

All new API surface goes in **app-server v2**; do not extend v1. Conventions:
- Payloads: `*Params` (request), `*Response`, `*Notification`. RPC methods as `<resource>/<method>`, resource singular (`thread/read`, `app/list`).
- `#[serde(rename_all = "camelCase")]` + `#[ts(rename_all = "camelCase")]` on wire types; keep Rust and TS renames aligned. Exception: config RPC payloads use snake_case to mirror `config.toml`.
- `#[ts(export_to = "v2/")]` on all v2 types. Never `#[serde(skip_serializing_if = "Option::is_none")]` on v2 payload fields (narrow exception for no-param client→server requests).
- Optional fields in `*Params`: `#[ts(optional = nullable)]`. Optional collections: `Option<Vec/HashMap>` + `#[ts(optional = nullable)]`, never `#[serde(default)]`.
- New list methods implement cursor pagination by default (`cursor`/`limit` in, `data`/`next_cursor` out). Timestamps are integer Unix seconds (`i64`) named `*_at`. Prefer plain `String` IDs at the boundary.
- After shape changes: `just write-app-server-schema`, then `just test -p codex-app-server-protocol`. Update `app-server/README.md` when behavior changes.

## Testing

- **Prefer integration tests over unit tests** for agent changes. Integration tests live in `core/suite` and use `test_codex` to stand up an instance. Features that change agent logic MUST add an integration test.
- Unit tests go in a dedicated `*_tests.rs` file referenced via `#[cfg(test)] #[path = "..._tests.rs"] mod tests;` — don't move existing inline test modules just to follow this.
- Use `pretty_assertions::assert_eq` (deep equals on whole objects, not field-by-field). Avoid mutating process env in tests — pass flags/dependencies from above.
- To spawn first-party binaries in tests, prefer `codex_utils_cargo_bin::cargo_bin("...")` over `assert_cmd`/`escargot`; for fixture resources use `codex_utils_cargo_bin::find_resource!` so paths resolve under both Cargo and Bazel runfiles (avoid `env!("CARGO_MANIFEST_DIR")`).
- **Snapshot tests (insta)** are required for any user-visible UI change (especially `codex-rs/tui`). Workflow: `just test -p codex-tui` → `cargo insta pending-snapshots -p codex-tui` → review `*.snap.new` → `cargo insta accept -p codex-tui` only when intentionally accepting. Install: `cargo install --locked cargo-insta`.
- Don't add tests for statically-defined values, negative tests for removed logic, or boilerplate tests that only assert experimental field markers.
- Core integration tests: use `core_test_support::responses` helpers — `mount_sse_once` + `responses::sse(vec![ev_*])`, `ResponseMock::single_request()`/`.requests()` for assertions, `wait_for_event` over the `_with_timeout` variant.
- Platform support: tests/features must support Linux, macOS, and Windows unless explicitly OS-specific; remote app-server/exec-server cross-OS configs are tested via the `$remote-tests` skill.

## TUI conventions (ratatui)

Full guide in `codex-rs/tui/styles.md`. In short: prefer Stylize helpers (`"text".dim()`, `.cyan()`, `.underlined()`); use `"text".into()` for spans and `vec![…].into()` for lines when the target type is obvious; `Span::styled` is fine for runtime-computed styles. **Never use `.white()`** — prefer the default foreground. Don't churn between equivalent forms. Wrap plain strings with `textwrap::wrap`; wrap ratatui `Line`s with helpers in `tui/src/wrapping.rs`; prefix lines via `prefix_lines` from `line_utils`.

## Docs layout (Nexus-specific)

`docs/` follows the DeepThink workflow convention (see the global `CLAUDE.md` for the full process): each requirement/dev task gets its own folder under `docs/prd/`, `docs/tech_solution/`, `docs/task_state/`, `docs/test_report/`; issue/bug postmortems go in `docs/issues/{YYYY-MM-DD}-{slug}.md` with the eight fixed sections. `docs/architecture/` holds the platform roadmap; `docs/codex_docs/` is a vendored copy of upstream Codex docs (`config.md`, `execpolicy.md`, `sandbox.md`, `skills.md`, `slash_commands.md`, etc.) — consult these when you need Harness behavior details, but do not add general product/user-facing docs to `docs/` (upstream Codex docs live elsewhere; the only exception is app-server API docs).

## Two things to keep straight

- **Codex did not open-source the model.** The Harness + integration interfaces are open; GPT-5.x is API/ChatGPT-only. Local/private models route through `ollama`/`lmstudio` providers (reduced capability).
- **Codex's approval mechanism cannot be lifted to the Web directly.** It's a local TUI popup blocking in-process. Enterprise "approve hours later on IM, Pod rebuilt in between" requires a self-built bridge that persists approvals to the DB and replays them on resume.
