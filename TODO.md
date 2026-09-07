# CLI Commands Testing To-Do List

This document tracks all CLI commands for `linkedin-cli-rust`. Commands that have been manually tested against the live browser/API are marked with `[x]`. Untested commands are marked with `[ ]`.

---

## 1. Authentication & Session Identity (`auth`)
- [x] `linkedin auth login` — Log in to LinkedIn using credentials or interactive session with 2FA/challenge handling.
- [x] `linkedin auth status` — Check session authentication state and current active profile.
- [x] `linkedin auth whoami` — Display the authenticated member's identity name and URN.
- [x] `linkedin auth add <name>` — Add and securely store credentials/cookies to native OS keyring.
- [x] `linkedin auth list` — List all configured profile sessions and identify the active session.
- [ ] `linkedin auth switch <name>` — Switch the active profile session.
- [ ] `linkedin auth remove <name>` — Remove a configured profile and its credentials from keyring.

---

## 2. Profile Management (`profile`)
- [x] `linkedin profile get <handle>` — Scrape member's full profile details (headline, location, positions, educations).
- [x] `linkedin profile contact <handle>` — Scrape member's contact info (websites, emails, phones, birthday, connected date).
- [x] `linkedin profile status <handle>` — Check connection degree/relationship status with a member.

---

## 3. Connection Management (`connection`)
- [x] `linkedin connection request <handle> [--note <text>]` — Send a connection invitation with optional note.

---

## 4. Messaging & Conversation Threads (`message`)
- [x] `linkedin message send <handle> [--text <text>] [-a|--attachment <path>]` — Send a direct message to a connection with optional text and file attachment.
- [x] `linkedin message send-batch --template <tpl> --recipients <file> [-a|--attachment <path>]` — Dispatch human-paced, jittered batch direct messages with optional per-recipient or default attachments.
- [x] `linkedin message thread <handle> [--limit <n>]` — Retrieve conversation history with a member.

---

## 5. Inbox (`inbox`)
- [x] `linkedin inbox list [--all] [--cursor <tok>] [--unread]` — List and filter conversation threads from the inbox with cursor-based and full-history pagination.

---

## 6. People Search (`search`)
- [x] `linkedin search people <keywords> [--network <degree>] [--location <loc>] [--hiring-for <title>]` — Search LinkedIn members with filters.

---

## 7. Invitations (`invite`)
- [x] `linkedin invite list [--sent] [--received] [--scroll <n>] [--limit <n>]` — List incoming or outgoing connection invitations.
- [x] `linkedin invite accept <handle>` — Accept a pending incoming invitation.
- [x] `linkedin invite withdraw <handle>` — Withdraw a pending sent invitation.
- [ ] `linkedin invite ignore <handle>` — Ignore a pending incoming invitation.

---

## 8. Post Publishing & Analytics (`post`)
- [ ] `linkedin post create --file <path.md> [--media <path>] [--visibility public|connections]` — Publish a post from a Markdown file.
- [ ] `linkedin post stats <activity_urn>` — View impression and engagement analytics for a post.
- [ ] `linkedin post draft add [--text <str> | --file <path>] [--media <path>] [--schedule <rfc3339>]` — Stage a post draft in local SQLite store.
- [ ] `linkedin post draft list [--status draft|scheduled|published|failed]` — View staged drafts by state.
- [ ] `linkedin post draft export --id <id> [--format cron|systemd]` — Export scheduled draft execution to crontab or systemd timer.

---

## 9. Session Management & Daemons (`session`)
- [ ] `linkedin session open [--name <session>] [--headed]` — Launch and register a long-running supervisor daemon instance.
- [ ] `linkedin session status [--name <session>]` — Check if a browser supervisor daemon is actively running and responsive.
- [ ] `linkedin session close [--name <session>]` — Gracefully terminate a running session daemon and clear registry.
- [ ] `linkedin session list` — List all registered supervisor sessions with PID and WebSocket endpoints.

---

## 10. Terminal UI (`tui`)
- [ ] `linkedin tui [--tab invitations|messages|drafts]` — Launch interactive split-pane terminal dashboard.

---

## 11. Debugging & CDP Evaluation (`debug`)
- [ ] `linkedin debug eval [--js <expr> | --script <expr>]` — Execute arbitrary JavaScript in live browser page context.

---

## 12. Utilities & Shell Integration
- [ ] `linkedin completions --shell <bash|zsh|fish|powershell>` — Generate shell autocompletion script.
- [ ] `linkedin manpage --out-dir <dir>` — Generate Troff manpages for documentation.
