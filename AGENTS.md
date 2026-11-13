# LinkedIn CLI Developer & Agent Architecture Guide

## Project Overview
Fast, Unix-composable LinkedIn command-line interface written in Rust. Automates LinkedIn workflows directly via the Chrome DevTools Protocol (CDP) using `chromiumoxide`.

---

## Architecture & Code Boundaries
- **Composition Root (`src/main.rs`)**: CLI argument parsing, session resolution, global flags (`--json`, `--url`, `--dry-run`), dependency wiring, error formatting, and process exit codes. No domain logic here.
- **Browser & Session Layer (`src/browser/`)**: Wraps Chromium process lifecycle, stealth evasions (`--disable-blink-features=AutomationControlled`), viewport pacing, and `BrowserSession` connection pooling.
- **Domain Layer (`src/domain/`)**: Pure business models and traits (e.g., `Profile`, `JobPosting`, `Conversation`, `InteractionRule`). Must NOT import `chromiumoxide` or CDP protocol crates.
- **Infrastructure Layer (`src/infra/`)**: Concrete implementations of domain traits executing CDP actions, DOM scrapers, and OS keyring credential storage.
- **Standard Unix I/O Contract**: Formatted business results (`table`, `json`, `ndjson`, `tsv`) are emitted strictly to `stdout`. All operational logs, diagnostics, and CDP traces route to `stderr` via `tracing`.

---

## Browser Session Lifecycle & Direct Navigation
- **Direct-Target Routing**: Commands operating on specific entities (`profile get`, `jobs get/save/unsave`, `debug eval --url`) must determine their destination URL in `src/main.rs` before calling `session.get_or_create_page(&target_url)`.
- **Avoid Intermediate Landing Pages**: Never navigate to generic landing pages (`/feed/`, `/jobs/`) and then perform client-side redirects. Direct navigation eliminates 5–10s of network latency, avoids unnecessary asset requests, and reduces anti-bot scoring.
- **Session Reuse & Keyring Cookies**: The CLI automatically extracts credentials and session cookies (`li_at`) from the native OS keyring and injects them into local Chromium sessions. Always ensure new commands utilize the existing `BrowserSession` rather than creating raw, unauthenticated CDP targets.

---

## Modern Web & React DOM Automation
- **Synthetic Event Propagation for Controlled Inputs**: Direct `.value = "..."` assignments fail silently on React/SDUI controlled components without updating component state or clearing validation errors (`"This field is required"`). Always dispatch synthetic events via the element prototype descriptor:
  ```javascript
  const desc = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value');
  desc.set.call(el, val);
  el.dispatchEvent(new Event('input', { bubbles: true }));
  el.dispatchEvent(new Event('change', { bubbles: true }));
  el.dispatchEvent(new Event('blur', { bubbles: true }));
  ```
- **Dynamic Opaque Controls**: Form controls with server-generated UUID values (e.g., ClearCompany or Workday ATS dropdowns) must be matched dynamically by normalized text (`option.text.toLowerCase().includes(...)`) rather than assuming static option values.
- **Headless In-Page File Uploads (`DataTransfer`)**: When automating file uploads, avoid triggering native OS file pickers which block headless Chromium execution. Instead, intercept the dynamically created `<input type="file">` and populate files using `DataTransfer` with in-memory decoded `Blob` data:
  ```javascript
  const blob = new Blob([byteArray], { type: 'application/pdf' });
  const file = new File([blob], filename, { type: 'application/pdf', lastModified: Date.now() });
  const dt = new DataTransfer();
  dt.items.add(file);
  input.files = dt.files;
  input.dispatchEvent(new Event('change', { bubbles: true }));
  input.dispatchEvent(new Event('input', { bubbles: true }));
  ```

---

## Agent Execution & Safety Protocol
- **Strict Read vs. Mutate Separation**:
  - Read-only commands (`profile get`, `jobs search`, `feed list`, `inbox list`, `auth status`) may be executed autonomously.
  - Mutating commands (`connection request`, `message send`, `feed react`, `feed comment`, `jobs save`) must first be verified with `-d, --dry-run` and presented to the human operator for explicit confirmation before live execution.
- **Multi-Step Review Gates**: When automating multi-step workflows (e.g. job applications, profile setup), navigate through information and questionnaire steps, stop at the final **Review** screen, and present the complete payload to the user in chat. Never autonomously click terminal submission buttons without explicit approval.
- **Interactive Tripwire Escalation**: If the anti-bot tripwire detects a checkpoint, CAPTCHA, or verification prompt, the CLI brings the browser window to the foreground, rings a system bell, and fires a desktop notification. The agent must NEVER attempt to solve or bypass challenges; pause execution immediately and notify the human operator to resolve it in the visible browser.

---

## Tooling & mise Workflows
- `mise run lint` — Runs `cargo clippy -- -D warnings`.
- `mise run test` — Executes unit and integration tests.
- `mise run run:headed` — Launches browser with visible UI for local debugging.
- `mise run run:debug` — Launches Chrome/Chromium directly with `--remote-debugging-port=9222` for external DevTools inspection.

---

## Coding Rules & Rust Idioms
- **No Unchecked Panics**: Never introduce `.unwrap()` or `.expect()`. Propagate errors with `?`.
- **Error Typing**: Domain and internal modules use typed `thiserror` enums (`AppError`). CLI root uses `anyhow::Result` / application error formatting.
- **Borrowing & Copies**: Borrow slices (`&str`, `&[T]`) for selectors, queries, and credentials. Minimize `.clone()`.
- **Resource Cleanup**: Ensure Chromium instances terminate cleanly on SIGINT/SIGTERM; prevent orphaned browser processes.
- **Wait Strategies**: Avoid arbitrary `tokio::time::sleep()`. Wait on explicit DOM nodes, network idle signals, or CDP events.

---

## Debugging & Troubleshooting Instructions
- **Diagnostic Logging**:
  - Operational logs route to `stderr` via `tracing`. Enable detailed logs with `RUST_LOG=debug` or scope to specific modules (e.g. `RUST_LOG=linkedin=debug,contact=debug,scraper=debug`).
  - Keep `stdout` dedicated exclusively to structured output (`--json`, `--ndjson`, `--table`, `--tsv`).
- **In-Process Telemetry & Diagnostic Flags (`--raw`)**:
  - Check `--raw --json` on subcommands (`profile contact <handle> --raw --json`, `profile get <handle> --raw --json`) to inspect raw data before UI filtering.
  - When debugging DOM scraping issues, inject diagnostic fields into in-page JavaScript evaluation payloads (e.g., `_debug_modal_text`, `_debug_modal_tag`, `_debug_anchors`) rather than blindly guessing CSS selectors.
- **Remote DevTools & Visual Troubleshooting**:
  - For visual checkpoints, run `mise run run:headed` or use `mise run run:debug` to launch Chromium with `--remote-debugging-port=9222`.
  - Use Chrome DevTools MCP tools (`list_pages`, `take_screenshot`, `evaluate_script`) on port 9222 to inspect live page layout, DOM state, and network responses out-of-band.
- **LinkedIn Frontend Gotchas & Patterns**:
  - **SDUI Lazy Hydration**: LinkedIn's SDUI renders empty container shells before resolving underlying GraphQL payloads. Never assume an element contains data immediately upon matching a selector; wait until child links, specific label text, or meaningful body length (> 60 chars) populate.
  - **Redirect URL Masking**: Outbound links in modals and profiles are wrapped with `https://www.linkedin.com/redir/redirect?url=...`. Always decode the `url` query parameter rather than filtering them out as `linkedin.com` links.
  - **Voyager REST Deprecations (HTTP 410)**: Legacy `/voyager/api/` endpoints (such as `/profileContactInfo`) may return HTTP 410 Gone. Maintain resilient fallback to in-page overlay navigation and DOM extraction.
  - **Profile Entity Ambiguity**: Voyager payloads often bundle both the authenticated viewer and the queried profile in the `included` array. Always verify `publicIdentifier == target_id` to avoid returning the viewer's profile instead of the target.
- **Verification Workflow**:
  1. `cargo clippy -- -D warnings` (Strict linter; zero warnings allowed).
  2. `cargo test` (Ensure all unit, domain, and integration tests pass).
  3. Run non-destructive live smoke tests with `--headed` and `-o json` (e.g. `profile get <handle>`, `jobs search <query>`).
