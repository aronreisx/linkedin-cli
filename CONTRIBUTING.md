# Contributing to LinkedIn CLI

Thank you for your interest in contributing to `linkedin-cli`! This project is an open-source, Unix-composable LinkedIn automation tool written in Rust.

---

## Development Setup

### Prerequisites
- **Rust Toolchain**: Stable Rust 1.80+ installed via [rustup](https://rustup.rs/).
- **Task Runner (Recommended)**: [`mise`](https://mise.jdx.dev/) is used for unified task orchestration.
- **Browser**: Chromium or Google Chrome installed locally.

### First-Time Build
```bash
# Clone the repository
git clone https://github.com/aronreisx/linkedin-cli.git
cd linkedin-cli

# Build debug binary
cargo build

# Run unit and integration tests
mise run test    # or: cargo test

# Run linter
mise run lint    # or: cargo clippy -- -D warnings
```

---

## Architecture Guidelines

1. **Standard Unix I/O Separation**:
   - `stdout` is reserved exclusively for formatted data (`table`, `json`, `tsv`, `ndjson`).
   - `stderr` receives all operational logging, tracing, and diagnostics via `tracing`.
2. **Domain Decoupling**:
   - Keep domain models and business logic in `src/domain/` completely decoupled from `chromiumoxide` and CDP protocol crates.
   - CDP operations belong in `src/infra/` and `src/browser/`.
3. **No Unchecked Panics**:
   - Never use `.unwrap()` or `.expect()` in non-test code. Propagate errors cleanly with `?`.
4. **Anti-Bot & Rate Limiting**:
   - Any automation or scraping routine that simulates human interaction must utilize human-paced jitter from `src/browser/stealth.rs`.
5. **Interactive Tripwires**:
   - The CLI detects anti-bot checkpoints (CAPTCHAs, verification pins) and surfaces them to the human operator. Never attempt to programmatically bypass security checkpoints.

---

## Pull Request Process

1. **Create a Feature Branch**:
   ```bash
   git checkout -b feat/my-new-command
   ```
2. **Ensure All Quality Checks Pass**:
   ```bash
   cargo fmt --check
   cargo clippy --all-targets -- -D warnings
   cargo test
   ```
3. **Submit Your PR**:
   - Write a clear, concise description of your changes.
   - Reference any relevant issues.
