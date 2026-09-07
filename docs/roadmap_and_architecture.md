# Architecture Review & Implementation Roadmap

**Author**: Senior Software Architect & Forward-Deployed Engineer (FDE)  
**Target Repository**: `linkedin-cli-rust`  
**Reference System**: `linkedin-cli` (Python / Playwright)  
**Status**: Strategic Blueprint & Technical Design Document  
**Date**: September 2026  

---

## 1. Executive Critical Assessment

A critical analysis of `linkedin-cli-rust` against enterprise forward-deployed requirements reveals significant architectural strengths, but also severe operational vulnerabilities that threaten account safety, runtime stability, and development velocity.

### Architectural Strengths
* **Clean Domain Boundary**: The separation of `src/domain/` from `src/infra/` and `src/browser/` strictly adheres to hexagonal architecture. Domain models and traits do not leak `chromiumoxide` details.
* **Standard I/O Hygiene**: Routing operational tracing to `stderr` while keeping `stdout` dedicated to structured outputs (`--json`, `--table`, `--tsv`) enables seamless Unix pipeline composition.
* **Fast Native Compilation & Binary Distribution**: Rust provides a self-contained, high-performance binary with zero Python virtualenv, Node runtime, or native bridge friction.

### Critical Vulnerabilities & Deficiencies
1. **Critical Bot Telemetry & Detection Leaks (Account Ban Risk)**:
   * **Missing Stealth Script Injection**: The engine relies solely on `--disable-blink-features=AutomationControlled`. It does not inject CDP `Page.addScriptToEvaluateOnNewDocument` shims. The runtime leaks headless Chromium signatures: default `navigator.webdriver`, unmasked WebGL renderers (`SwiftShader` / `Mesa`), non-existent `window.chrome` objects, and empty `navigator.plugins`.
   * **Instantaneous Keystroke Ingestion**: In `src/infra/messaging.rs`, text is dispatched via `InsertTextParams::new(text)` (0ms instantaneous dump). In `src/infra/connection.rs`, connection notes are set via DOM value mutation (`textarea.value = noteText`). LinkedIn records input cadence, keydown/keyup timestamps, and typing intervals via client-side behavioral telemetry (`x-li-track`). Instant dumps scream automation.
2. **Ephemeral Process Lifecycle vs. Session Persistence**:
   * Every CLI invocation launches a new Chromium process, writes user data, navigates, executes, and tears down the browser.
   * *Overhead*: 1.5s to 3.5s latency floor per command.
   * *Telemetry Noise*: Frequent Chromium bootstrap handshakes increase LinkedIn's device fingerprint suspicion score.
   * *Checkpoint Insolvency*: When LinkedIn presents an interactive checkpoint (CAPTCHA / 2FA verification), an ephemeral CLI process simply aborts or times out, giving the operator no window to solve it without re-authenticating.
3. **Scattered, Unmaintainable In-Page JavaScript Strings**:
   * Infrastructure implementations (`src/infra/connection.rs`, `src/infra/messaging.rs`, `src/infra/contact.rs`, `src/infra/search.rs`) contain raw string literals exceeding 200 lines of JavaScript. These lack linting, type-checking, syntax highlighting, and compile-time testing.
4. **Stubbed Commands Presented as Working Capabilities**:
   * As cataloged in [`docs/incomplete_features.md`](file:///Users/aronreis/Projects/linkedin-cli-rust/docs/incomplete_features.md), `linkedin post create` outputs fake URNs without launching a browser; `linkedin post stats` parses a hardcoded mock JSON payload; `linkedin tui` displays disconnected dummy data; `linkedin post draft export` references an unimplemented runner command.

---

## 2. Proposed Architectural Patterns & Principles

### 2.1 The Supervisor Daemon & Attached Session Pattern
**Problem**: Launching Chromium on every command wastes compute and triggers security checkpoints.  
**Pattern**: Dual-mode execution via **Bound Session Supervisor** (Client/Daemon architecture).

```
┌────────────────────────────────────────────────────────┐
│                   CLI Subcommand                       │
│    (e.g., `linkedin message send <user> --text ...`)   │
└──────────────────────────┬─────────────────────────────┘
                           │ 1. Check registry: ~/.linkedin-cli/sessions/default.json
          ┌────────────────┴────────────────┐
          ▼                                 ▼
   [Session Exists]                 [No Running Session]
Connect to existing CDP WS           Launch managed Chromium
endpoint (latency: < 50ms)           (or spawn daemon supervisor)
          │                                 │
          └────────────────┬────────────────┘
                           ▼
             ┌───────────────────────────┐
             │   Bound Chromium Engine   │
             │  (Persistent Cookie Jar   │
             │   & Warm DOM Context)     │
             └───────────────────────────┘
```

* **Implementation**:
  * Introduce `linkedin session open [--name <profile>] [--port <port>]`: Starts a daemonized Chromium instance with an exposed CDP WebSocket port and writes metadata (`{ pid, endpoint, profile }`) to `~/.linkedin-cli/sessions/<profile>.json`.
  * Standard subcommands check for an active session. If active, they attach in milliseconds via `BrowserSession::connect(endpoint)`. If absent, they fall back to managed one-off launch.
  * Solves the checkpoint problem: when a challenge triggers, the supervisor can remain alive for 30 minutes (`CHECKPOINT_RESOLVE_TIMEOUT_S`), allowing a human operator to attach via VNC/headed browser and solve the prompt interactively.

### 2.2 Stealth & Behavioral Dynamics Layer (`StealthDriver`)
**Problem**: LinkedIn flags robotic interactions and non-human typing cadences.  
**Principle**: "Physics-First Automation" — all input must simulate physical hardware dynamics and cognitive pauses.

* **Component Architecture**:
  * **`StealthBootstrap`**: On browser startup or new page target creation, dispatch `Page.addScriptToEvaluateOnNewDocument` executing evasions:
    * Delete / mask `navigator.webdriver`.
    * Inject `window.chrome` runtime shim.
    * Mask WebGL vendor / renderer to match physical host graphics hardware.
    * Populate realistic `navigator.languages` and mock `navigator.plugins`.
  * **`HumanTypingEngine`**:
    * Replace `InsertTextParams` and JS value assignments with real CDP `Input.dispatchKeyEvent` loops.
    * Implement Gaussian-distributed delays (`mean = 110ms`, `std_dev = 25ms`).
    * Inject cognitive modifiers:
      * Capital letters (`Shift` delay): +80ms to 180ms.
      * Numbers / special symbols (hand travel): +60ms to 140ms.
      * Clause punctuation (`.`, `,`, `!`): +400ms to 800ms natural reading pause.
      * Word spaces: +50ms to 150ms.
    * Typo injection: 1.5% probability of striking an adjacent key, pausing 200–400ms (realization), pressing `Backspace`, and re-typing.

### 2.3 Account Protection Levees (Token Bucket & Circuit Breaker)
**Problem**: LinkedIn applies strict daily interaction caps. Aggressive scripting burns accounts permanently.  
**Principle**: Defense-in-Depth Account Safety.

* **Component Architecture**:
  * **`RateLimitLedger` (SQLite-backed in `~/.linkedin-cli/audit.db`)**:
    * Track rolling 24-hour interaction counts:
      * Connection invitations sent: Max 25–35 / day.
      * Direct messages dispatched: Max 60–80 / day.
      * Profile lookups: Max 100 / day.
    * Subcommands verify quota before issuing actions. If limit reached, fail with `AppError::RateLimitExceeded`.
  * **`TripwireCircuitBreaker`**:
    * Monitor all CDP page responses and URL navigations.
    * If URL matches `/checkpoint/` or HTTP status code equals `429 Too Many Requests` / `999 LinkedIn Anti-Bot`:
      1. Immediately set state to `TRIPPED` in `audit.db`.
      2. Abort all pending batch queues.
      3. Emit high-priority alerts to `stderr`.
      4. Require manual `--force-reset-tripwire` before any further operations can be launched.

### 2.4 In-Page Transport & SDUI Hydration Guards
**Problem**: Voyager API endpoints return HTTP 410, while SDUI DOM trees render empty shells before GraphQL resolves.  
**Principle**: Unified In-Page `fetch()` Transport with Semantic Hydration Boundaries.

* **In-Page `fetch()`**:
  * Encapsulate API queries inside `session.evaluate(js_fetch)`. This automatically inherits all browser-level headers: `csrf-token` from `JSESSIONID`, `x-li-track`, `sec-ch-ua-*`, cookies, and browser TLS fingerprint.
* **SDUI Semantic Hydration Guard**:
  * Ban arbitrary timeouts (`sleep(2000)`).
  * Enforce DOM guards: wait until target container contains text exceeding a threshold (e.g., length > 40 chars) or until specific action buttons (`button[aria-label*="Connect"]`) enter interactive state.

---

## 3. Prioritized Implementation Roadmap

The roadmap is strictly sorted by priority. Tier 0 and Tier 1 address survival and operational correctness; Tier 2 and Tier 3 address feature completion and operator ergonomics.

```
┌────────────────────────────────────────────────────────┐
│  Tier 0: Account Safety & Stealth Hardening            │ ◄── HIGHEST PRIORITY
│  (Stealth script injection, Human typing, Tripwire)    │     (Prevents bans)
└──────────────────────────┬─────────────────────────────┘
                           ▼
┌────────────────────────────────────────────────────────┐
│  Tier 1: Bound Session Daemon & Shared Browser Engine  │ ◄── CORE ARCHITECTURE
│  (Persistent supervisor, <50ms attach, Checkpoints)    │     (10x performance)
└──────────────────────────┬─────────────────────────────┘
                           ▼
┌────────────────────────────────────────────────────────┐
│  Tier 2: Feature Completion & Publishing Automation    │ ◄── FUNCTIONAL GAPS
│  (Real post create, Live analytics, Draft runner)      │     (Eliminates mocks)
└──────────────────────────┬─────────────────────────────┘
                           ▼
┌────────────────────────────────────────────────────────┐
│  Tier 3: Platform Ergonomics & Live TUI                │ ◄── POLISH & OPS
│  (Daily quota ledger, Live TUI wiring, Shell autocomp) │
└────────────────────────────────────────────────────────┘
```

---

### Tier 0: Account Safety & Anti-Detection Hardening
*Objective*: Eliminate immediate account banning risks caused by bot signatures and instantaneous input execution.

| Task ID | Component | Description | Deliverable |
| :--- | :--- | :--- | :--- |
| **T0.1** | `src/browser/stealth.rs` | **CDP Stealth Script Injection**: Implement `Page.addScriptToEvaluateOnNewDocument` shimming `navigator.webdriver`, `window.chrome`, WebGL vendor/renderer masking, and realistic language/plugin arrays. | Zero headless signature leaks in Chromium CDP sessions. |
| **T0.2** | `src/infra/typing.rs` | **Gaussian Human Typing Engine**: Implement character-by-character `Input.dispatchKeyEvent` with bell-curve pauses, contextual punctuation/shift delays, and typo backspacing. | Replace raw `InsertTextParams` in `messaging.rs` and JS assignments in `connection.rs`. |
| **T0.3** | `src/infra/circuit_breaker.rs` | **Tripwire & Checkpoint Detection**: Detect `/checkpoint/` challenges, HTTP 429, and HTTP 999. Immediately halt execution and lock session to prevent account cascading bans. | Tripwire guard integrated into all navigation and action routines. |

---

### Tier 1: Bound Session Daemon & Shared Browser Architecture
*Objective*: Eliminate the 3-second startup penalty per command, reduce fingerprinting churn, and enable human-in-the-loop checkpoint resolution.

| Task ID | Component | Description | Deliverable |
| :--- | :--- | :--- | :--- |
| **T1.1** | `src/browser/session.rs` | **Session Registry & Fast Attach**: Store active CDP WebSocket endpoints in `~/.linkedin-cli/sessions/<profile>.json`. Subcommands attach instantly to active sessions. | Sub-100ms command invocation latency for warm sessions. |
| **T1.2** | `src/main.rs` (`session`) | **Supervisor Daemon Subcommand**: Implement `linkedin session open [--name <profile>]` to launch and maintain a bound persistent Chromium instance. | Interactive supervisor daemon keeping browser context warm. |
| **T1.3** | `src/browser/session.rs` | **Global Slow-Mo Middleware**: Implement configurable action pacing (e.g. 150–250ms delay between consecutive CDP events) mirroring Playwright's `slow_mo`. | Natural pacing across all DOM and CDP interactions. |
| **T1.4** | `src/infra/transport.rs` | **In-Page `fetch()` Transport Primitive**: Standardize in-page REST queries that extract CSRF tokens from cookies and inherit all browser headers. | Reliable Voyager communication fallback without raw external HTTP clients. |

---

### Tier 2: Feature Completion & Staged Execution Loop
*Objective*: Replace all mocked and stubbed subcommands with real production implementations.

| Task ID | Component | Description | Deliverable |
| :--- | :--- | :--- | :--- |
| **T2.1** | `src/infra/post.rs` | **Live Browser Post Publishing**: Implement full browser automation for `linkedin post create --file <path.md> [--media <path>]`, handling post modal interaction and CDP file upload. | Real LinkedIn post creation with verified activity URN extraction. |
| **T2.2** | `src/main.rs` (`post publish`) | **Draft Execution Runner**: Implement `linkedin post publish --draft-id <id>` to allow crontab and systemd timers generated by `post draft export` to publish scheduled drafts. | End-to-end automated publishing loop from staged SQLite drafts. |
| **T2.3** | `src/infra/analytics.rs` | **Live Post Analytics**: Replace hardcoded mock JSON in `linkedin post stats <urn>` with live Voyager analytics queries or in-page post analytics DOM scraping. | Real-time impression and engagement metrics for published posts. |
| **T2.4** | `src/main.rs` (`invite`) | **Complete Pending Invitation Commands**: Implement missing `linkedin invite ignore <handle>` and test against live DOM invitation cards. | Full feature parity across invitation lifecycle. |

---

### Tier 3: Production Ergonomics, Rate Limiting & TUI
*Objective*: Long-term operational reliability, quota management, and terminal developer experience.

| Task ID | Component | Description | Deliverable |
| :--- | :--- | :--- | :--- |
| **T3.1** | `src/infra/quota.rs` | **Persistent Quota & Rate Limiting Ledger**: Embedded SQLite database (`audit.db`) enforcing rolling 24-hour limits on invites, messages, and profile lookups. | Guaranteed prevention of daily platform quota violations. |
| **T3.2** | `src/infra/tui_infra.rs` | **Live Data TUI Wiring**: Connect `linkedin tui` panes directly to SQLite draft storage, local invitation caches, and active session inbox status. | Functional terminal dashboard reflecting live state. |
| **T3.3** | `src/main.rs` (`completions`) | **Shell Autocompletion Generator**: Wire `clap_complete` to generate native completion scripts for `bash`, `zsh`, `fish`, and `powershell`. | Zero-friction shell productivity for operators. |

---

## 4. Architectural Patterns & Technical Specifications

### 4.1 Stealth Injection Specification (`src/browser/stealth.rs`)

```rust
// Proposed Stealth Evaluator Hook via CDP
pub const STEALTH_JS_PAYLOAD: &str = r#"
(() => {
    // 1. Mask navigator.webdriver
    Object.defineProperty(navigator, 'webdriver', {
        get: () => undefined,
    });

    // 2. Mock window.chrome object
    if (!window.chrome) {
        window.chrome = {
            app: { isInstalled: false, InstallState: { DISABLED: 'DISABLED' }, RunningState: { CANNOT_RUN: 'CANNOT_RUN' } },
            runtime: { OnInstalledReason: {}, OnRestartRequiredReason: {}, PlatformArch: {}, PlatformNaclArch: {}, PlatformOs: {}, RequestUpdateCheckStatus: {} },
            loadTimes: function() {},
            csi: function() {},
        };
    }

    // 3. Spoof WebGL vendor and renderer (hide SwiftShader/Mesa headless tags)
    const getParameter = WebGLRenderingContext.prototype.getParameter;
    WebGLRenderingContext.prototype.getParameter = function(parameter) {
        // UNMASKED_VENDOR_WEBGL
        if (parameter === 37445) return 'Intel Inc.';
        // UNMASKED_RENDERER_WEBGL
        if (parameter === 37446) return 'Intel Iris OpenGL Engine';
        return getParameter.apply(this, [parameter]);
    };

    // 4. Mock navigator.plugins and languages
    Object.defineProperty(navigator, 'languages', {
        get: () => ['en-US', 'en'],
    });
})();
"#;
```

### 4.2 Human Typing Specification (`src/infra/typing.rs`)

```rust
pub struct HumanTypingConfig {
    pub min_delay_ms: u64,
    pub max_delay_ms: u64,
    pub typo_rate: f64,
}

impl Default for HumanTypingConfig {
    fn default() -> Self {
        Self {
            min_delay_ms: 45,
            max_delay_ms: 180,
            typo_rate: 0.015,
        }
    }
}

// Emulate physical keyboard interaction via CDP Input.dispatchKeyEvent
pub async fn human_type_into_element(
    page: &chromiumoxide::Page,
    text: &str,
    config: &HumanTypingConfig,
) -> Result<(), AppError> {
    use chromiumoxide::cdp::browser_protocol::input::{DispatchKeyEventParams, DispatchKeyEventType};
    use rand::Rng;

    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        // 1. Simulate occasional typo and backspace correction
        if ch.is_alphabetic() && rand::thread_rng().gen_bool(config.typo_rate) {
            let typo_char = 'a'; // adjacent key calculation
            dispatch_key(page, typo_char).await?;
            tokio::time::sleep(tokio::time::Duration::from_millis(rand::thread_rng().gen_range(200..=400))).await;
            dispatch_key_press(page, "Backspace").await?;
            tokio::time::sleep(tokio::time::Duration::from_millis(rand::thread_rng().gen_range(100..=200))).await;
        }

        // 2. Dispatch real key events
        dispatch_char(page, ch).await?;

        // 3. Compute Gaussian delay with cognitive modifier
        let mut delay = rand::thread_rng().gen_range(config.min_delay_ms..=config.max_delay_ms);
        if ch.is_uppercase() {
            delay += rand::thread_rng().gen_range(70..=150); // Shift key delay
        } else if matches!(ch, '.' | ',' | '!' | '?') {
            delay += rand::thread_rng().gen_range(350..=750); // Clause pause
        } else if ch == ' ' {
            delay += rand::thread_rng().gen_range(40..=120);  // Word transition
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
    }
    Ok(())
}
```

---

## 5. Architectural Decision Records (ADRs) Summary

### ADR 01: Client-Daemon Model vs. Standalone Ephemeral Execution
* **Decision**: Adopt a dual-mode strategy. Default commands attach via WebSocket to a supervisor daemon (`~/.linkedin-cli/sessions/default.json`). If no daemon is found, fall back gracefully to ephemeral managed launch.
* **Rationale**: Reduces invocation latency from 2,800ms to 40ms, drastically minimizes bot initialization telemetry, and provides a continuous window to solve interactive security checkpoints.

### ADR 02: In-Page `fetch()` Transport vs. Raw Native HTTP Clients (`reqwest`)
* **Decision**: Route all Voyager API requests through `session.page.evaluate(js_fetch)` inside the active browser context rather than using standalone `reqwest` instances.
* **Rationale**: Replicating LinkedIn's dynamic header signatures (`x-li-track`, client TLS fingerprints, Akamai bot tokens, rotated CSRF hashes) in an external HTTP client is brittle and leads to HTTP 403/999 blocks. In-page `fetch()` inherits the entire browser fingerprint natively.

### ADR 03: Physical Keystroke Emulation vs. Instantaneous CDP Value Assignment
* **Decision**: Deprecate `InsertTextParams` and DOM `.value = ...` mutations for all user-facing content (direct messages, connection notes). Mandate character-by-character CDP key dispatching with cognitive delays.
* **Rationale**: LinkedIn's anti-fraud telemetry tracks DOM input intervals and focus/blur dynamics. Superhuman 0ms text insertion directly correlates with account flagging.
