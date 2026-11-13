# LinkedIn CLI (`linkedin`)

Fast, Unix-composable LinkedIn command-line interface written in Rust. Automates LinkedIn workflows directly via the Chrome DevTools Protocol (CDP) using `chromiumoxide`.

---

## Overview & Architecture

- **Clean Unix I/O Separation**: Formatted results (`table`, `json`, `ndjson`, `tsv`) are emitted strictly to `stdout`. All operational logs and tracing output stream to `stderr`.
- **Anti-Bot Human Jitter & Interactive Tripwire**: Configurable randomized delays and typing cadences to mirror human behavior. Built-in tripwire detection automatically brings the browser window to the foreground, emits an audible chime, and fires a desktop notification when CAPTCHAs or security checkpoints appear.
- **Keyring & Profile Sandbox**: Credentials and session cookies (`li_at`) securely stored in the native OS keyring; Chromium user data directories are isolated per profile under `~/.linkedin-cli/profiles/<session>`.
- **Direct-Target Page Routing**: `profile`, `jobs get/save/unsave`, and `debug eval --url` bypass intermediate landing page redirects, opening dedicated purpose-matched tabs directly to minimize CDP latency and bot detection surface.
- **Domain Decoupling**: Pure business domain logic separated from CDP infrastructure primitives.

---

## Toolchain & Tasks

This project uses [`mise`](https://mise.jdx.dev/) and standard `cargo`:

```bash
# Build release binary
cargo build --release

# Run tests
mise run test          # or: cargo test

# Lint with Clippy (zero warnings enforced)
mise run lint          # or: cargo clippy -- -D warnings

# Run headless CLI
mise run run -- <command>

# Run in headed mode (visible browser UI)
mise run run:headed -- <command>

# Launch standalone Chrome with remote debugging on :9222 (for MCP / DevTools)
mise run run:debug
```

---

## Global Options

All commands support these global options:

| Flag | Description |
|------|-------------|
| `-o, --output <format>` | Output format: `table` (default), `json`, `ndjson`, `tsv` |
| `--json` | Convenience shortcut for `--output json` |
| `-d, --dry-run` | Simulates mutating operations with zero network requests |
| `-q, --quiet` | Suppresses non-essential `stderr` diagnostics |
| `--headed` | Runs Chromium with a visible browser window |
| `--session <name>` | Session profile to use (default: `$LINKEDIN_CLI_SESSION` or `default`) |
| `--user-data-dir <path>` | Custom browser user data directory |
| `--cdp-url <url>` | Connect to an existing CDP instance (e.g. `http://127.0.0.1:9222`) |
| `--slow-mo <ms>` | Additional delay in milliseconds between browser operations (env: `$LINKEDIN_SLOW_MO`) |

---

## Authentication & Profiles

Store credentials in your system keyring or load them from environment variables (`LINKEDIN_USERNAME`, `LINKEDIN_PASSWORD`):

```bash
# Check current session login status
linkedin auth status

# Identify active authenticated user
linkedin auth whoami

# Add a named profile with credentials to keyring
linkedin auth add work --username user@example.com --password "secret"

# List stored profiles
linkedin auth list

# Switch active profile
linkedin auth switch work

# Delete stored profile
linkedin auth remove work
```

---

## Command Examples

### Profile & Contact Info
```bash
# Fetch member profile details
linkedin profile get alice-smith

# Fetch member profile details with untouched internal Voyager JSON
linkedin profile get alice-smith --raw --json

# Fetch member contact information (email, phone, websites)
linkedin profile contact alice-smith

# Check connection status with a member
linkedin profile status alice-smith
```

### Connections & Invitations
```bash
# Send a connection request with a personalized note (max 300 chars)
linkedin connection request alice-smith --note "Hi Alice, let's connect!"

# List pending received invitations
linkedin invite list --received

# Accept or ignore invitations directly
linkedin invite accept alice-smith
linkedin invite ignore bob-jones
```

### Messaging & Inbox
```bash
# Send a direct message
linkedin message send alice-smith --text "Thanks for connecting!"

# Inspect conversation thread history
linkedin message thread alice-smith --limit 20

# Filter inbox for unread conversations (defaults to ~20 if --limit is omitted)
linkedin inbox list --unread --limit 10

# Cursor-based pagination: fetch page 2 using the token from page 1
linkedin inbox list --cursor "<next_cursor_token>" --limit 20

# Paginate through complete inbox history (respecting anti-bot human jitter)
linkedin inbox list --all --limit 50

# Batch messaging with template variables and rate-limiting jitter
linkedin message send-batch \
  --template "Hi {{ first_name }}, enjoyed your post about {{ topic }}!" \
  --recipients recipients.json \
  --max-batch 15 \
  --jitter-min 45 \
  --jitter-max 120
```

### People Search
```bash
# Search people with filters (network, location, and hiring intent)
linkedin search people "Engineering Manager" \
  --network second \
  --location US \
  --actively-hiring \
  --hiring-for "Frontend Engineer" \
  --page 1
```

### Content Publishing & Analytics
```bash
# Publish a post from a Markdown file
linkedin post create --file post.md --media image.png --visibility public

# View engagement statistics and impression metrics
linkedin post stats urn:li:activity:1234567890
```

### Timeline & Feed Interactions
```bash
# Read recent or top posts from the main feed
linkedin feed list --limit 10 --sort recent

# Read recent posts from a specific member's public feed
linkedin feed member alice-smith --limit 5

# Inspect a specific post and its author details
linkedin feed get urn:li:activity:7123456789012345678

# List comments on a specific post
linkedin feed comments urn:li:activity:7123456789012345678 --limit 10

# React to a post (like, celebrate, support, love, insightful, funny)
linkedin feed react urn:li:activity:7123456789012345678 --type insightful

# Remove your reaction from a post
linkedin feed unreact urn:li:activity:7123456789012345678

# Post a comment (up to 1,250 characters, typed with human cadence)
linkedin feed comment urn:li:activity:7123456789012345678 --text "Great insights, thank you for sharing!"

# Repost instantly or quote repost with your own perspective
linkedin feed repost urn:li:activity:7123456789012345678
linkedin feed repost urn:li:activity:7123456789012345678 --text "Key takeaways from this discussion..."
```

### Job Search & Saved Opportunities
```bash
# Search job postings with faceted filters (workplace, timeframe, experience, Easy Apply)
linkedin jobs search "Staff AI Engineer" \
  --location "Remote" \
  --workplace remote,hybrid \
  --posted past-week \
  --experience mid-senior,director \
  --easy-apply \
  --limit 20 \
  --json

# Inspect full job posting description, hiring team recruiter, salary, and required skills
linkedin jobs get 4123456789 --json

# Save a job posting to your saved jobs list
linkedin jobs save 4123456789

# Remove a job from your saved jobs list
linkedin jobs unsave 4123456789

# List your saved jobs
linkedin jobs saved --limit 10 --json
```

#### Full-Cycle Job Application & Outreach Workflow
Pair job scouting with recruiter outreach to maximize response rates:
```bash
# 1. Search opportunities with faceted filters
linkedin jobs search "Applied AI Engineer" --location "Remote" --easy-apply --limit 5 --json

# 2. Inspect full requirements, salary, and hiring company
linkedin jobs get 4485518487 --json

# 3. Locate lead technical recruiters at the employer
linkedin search people "Inviso recruiter" --page 1 --json

# 4. Dispatch a targeted connection request (max 300 chars, dry-run first)
linkedin connection request markssilver \
  --note "Hi Mark, I just applied to Inviso's Applied AI Engineer role. With 10+ yrs in backend platforms and deep production experience building agent runtimes, LLM evaluation, and RAG systems, I'd love to connect. Best, Aron" \
  --dry-run
```

### Post Drafts & Scheduling
```bash
# Stage a post draft with a scheduled publish date
linkedin post draft add --file announcement.md --schedule "2026-09-10T15:00:00Z"

# List staged drafts
linkedin post draft list --status scheduled

# Export scheduled draft as a cron line or systemd timer
linkedin post draft export --id <draft-id> --format cron
linkedin post draft export --id <draft-id> --format systemd
```

### Browser Session Management & Daemons
```bash
# Launch a bound session supervisor daemon (blocking until SIGINT)
linkedin session open --name work --headed

# Check status of an active session daemon
linkedin session status --name work

# List all active registered supervisor sessions and their CDP WebSocket endpoints
linkedin session list

# Gracefully terminate an active session daemon
linkedin session close --name work
```
Subsequent commands (e.g. `linkedin --session work profile get ...`) automatically detect the active daemon and fast-attach via CDP without launching new Chromium instances.

### Page Evaluation & Debugging
```bash
# Evaluate arbitrary JavaScript expression in the live browser tab
linkedin debug eval "document.title"
linkedin debug eval --js "Array.from(document.querySelectorAll('a')).map(a => a.href)"

# Navigate to a specific URL first, then evaluate (ideal for inspecting dynamic forms or external ATS views)
linkedin debug eval --url "https://www.linkedin.com/jobs/view/4485518487/" \
  '(() => ({ title: document.title, hasEasyApply: !!document.querySelector("button[aria-label*=\"Easy Apply\"]") }))()'
```

### Interactive Terminal UI (TUI)
```bash
# Launch full terminal UI split-pane dashboard
linkedin tui

# Launch directly into a specific tab (invitations, messages, drafts)
linkedin tui --tab messages
```

### Shell Completions & Manpages
```bash
# Generate shell autocompletion script
linkedin completions --shell zsh > ~/.zfunc/_linkedin

# Generate manpages
linkedin manpage --out-dir /usr/local/share/man/man1
```

---

## Unix Composability

Filter and process streaming NDJSON output using tools like `jq` and `fzf`:

```bash
# Extract unread sender names via jq
linkedin inbox list --unread -o ndjson | jq -r '.sender'

# Select an invitation interactively with fzf and accept it
linkedin invite list --received -o ndjson \
  | jq -r '.[] | .handle' \
  | fzf \
  | xargs -I {} linkedin invite accept {}
```

---

## Dry-Run Testing

Verify command logic and variable rendering without executing browser actions:

```bash
linkedin message send alice-smith --text "Test" --dry-run
```

---

## AI Agent Skill

This package includes a dedicated AI agent skill in [`skill/`](./skill/SKILL.md) for Antigravity, Claude Code, Cursor, and custom agentic runners.

The skill encapsulates:
- Best practices and safety protocols (such as mandatory user confirmation for mutating operations).
- Anti-bot pacing guidelines and tripwire challenge recovery.
- Atomic recipes for profile reconnaissance, inbox spot-checks, invite triage, and one-off messaging without requiring the full background orchestrator.

