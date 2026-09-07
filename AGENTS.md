# CLI-based LinkedIn Automation Assistant

## Project Overview
A CLI for automating LinkedIn workflows.

## Architecture & Code Boundaries
- **Composition Root (`src/main.rs`)**: CLI parsing, dependency assembly, error handling, and process exit codes. No domain logic here.
- **Browser Layer (`src/browser/`)**: Wraps Chromium lifecycle, viewport management, and anti-detection flags (`--disable-blink-features=AutomationControlled`).
- **Domain Layer (`src/domain/`)**: Pure traits and business types (e.g., `Profile`, `InteractionRule`). Must NOT import `chromiumoxide`.
- **Infrastructure (`src/infra/`)**: Concrete trait implementations executing CDP actions on `chromiumoxide::Page`.
- **Standard I/O Separation**: All operational logs go to `stderr` via `tracing`. Only final structured data (JSON/text) routes to `stdout`.

## Tooling & mise Workflows
Toolchain is strictly Rust. MCP execution is handled externally by the local client config (`~/.gemini/config/mcp_config.json`).
- `mise run lint` — Runs `cargo clippy -- -D warnings`.
- `mise run test` — Executes unit and integration tests.
- `mise run run:headed` — Launches browser with visible UI for local debugging.
- `mise run run:debug` — Launches Chrome/Chromium directly with `--remote-debugging-port=9222` for external MCP / DevTools inspection.

## Coding Rules & Rust Idioms
- **No Unchecked Panics**: Never introduce `.unwrap()` or `.expect()`. Use the `?` operator.
- **Error Typing**: Internal libraries/modules use `thiserror` enums. CLI root uses `anyhow::Result`.
- **Borrowing & Copies**: Borrow slices (`&str`, `&[T]`) for selectors, queries, and credentials. Minimize `.clone()`.
- **Resource Cleanup**: Ensure Chromium instances terminate cleanly on SIGINT/SIGTERM; prevent orphaned browser processes.
- **Wait Strategies**: Avoid arbitrary `tokio::time::sleep()`. Wait on explicit DOM nodes, network idle signals, or CDP events.

## Agent Behavior & Code Generation Instructions
- Define domain traits in `src/domain/` before writing browser automation steps.
- When generating automation routines, always add anti-bot randomized human jitter between interactions.
- Avoid exposing LinkedIn credentials in CLI arguments; source them from environment variables or secure keyrings.
- Remote debugging is decoupled from the CLI: use `mise run run:debug` to launch Chrome independently when agent inspection is required.

## Rate Limiting & Account Protection
- **Account Safety**: During development tests and live interactions, exercise extreme caution and avoid aggressive or repetitive automation to prevent account flagging or permanent bans.
- **Pacing & Delays**: Always enforce human-paced jitter between requests and interactions. Never disable or bypass jitter for convenience in testing.
- **Safe Development Testing**: Prefer non-destructive read operations (`whoami`, `status`, isolated `profile` queries) over write operations (`connect`, bulk `message`, `invites --withdraw`). Avoid tight loops or hammering endpoints.

## Debugging & Troubleshooting Instructions
- **Diagnostic Logging**:
  - Operational logs route to `stderr` via `tracing`. Enable detailed logs with `RUST_LOG=debug` or scope to specific modules (e.g. `RUST_LOG=linkedin=debug,contact=debug,scraper=debug`).
  - Keep `stdout` dedicated exclusively to structured output (`--json`, `--ndjson`, `--table`, `--tsv`).
- **In-Process Telemetry & Diagnostic Flags (`--raw`)**:
  - Always check `--raw --json` on subcommands (`profile contact <handle> --raw --json`, `profile get <handle> --raw --json`) to inspect the raw data structure returned by LinkedIn before UI filtering.
  - When debugging DOM scraping issues, inject diagnostic fields into in-page JavaScript evaluation payloads (e.g., `_debug_modal_text`, `_debug_modal_tag`, `_debug_anchors`) rather than blindly guessing CSS selectors.
- **Remote DevTools & Visual Troubleshooting**:
  - For visual checkpoints (CAPTCHA, 2FA prompt screens, bot challenges, or session drops), run `mise run run:headed` or use `mise run run:debug` to launch Chromium with `--remote-debugging-port=9222`.
  - Use the Chrome DevTools MCP tools (`list_pages`, `take_screenshot`, `evaluate_script`) on port 9222 to inspect live page layout, DOM state, and network responses out-of-band.
- **LinkedIn Frontend Gotchas & Patterns**:
  - **SDUI Lazy Hydration**: LinkedIn's SDUI renders empty container shells (e.g. `<dialog data-testid="dialog">`) before resolving underlying GraphQL payloads. Never assume an element contains data immediately upon matching a selector; wait until child links, specific label text, or meaningful body length (> 60 chars) populate.
  - **Redirect URL Masking**: Outbound links in modals and profiles are wrapped with `https://www.linkedin.com/redir/redirect?url=...`. Always decode the `url` query parameter rather than filtering them out as `linkedin.com` links.
  - **Voyager REST Deprecations (HTTP 410)**: Legacy `/voyager/api/` endpoints (such as `/profileContactInfo`) may return HTTP 410 Gone. Always maintain a resilient fallback to in-page overlay navigation and DOM extraction.
  - **Profile Entity Ambiguity**: Voyager payloads often bundle both the authenticated viewer and the queried profile in the `included` array. Always verify `publicIdentifier == target_id` to avoid returning the viewer's profile instead of the target.
- **Verification Workflow**:
  - 1. `cargo clippy -- -D warnings` (Strict linter; no warnings allowed).
  - 2. `cargo test` (Ensure all unit, domain, and integration tests pass).
  - 3. Run non-destructive live smoke tests with `--headed` and `-o json` (e.g. `profile get <handle>`, `profile contact <handle>`).

