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
