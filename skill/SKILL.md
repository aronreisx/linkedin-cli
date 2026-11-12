---
name: linkedin-quick-actions
description: Fast, targeted LinkedIn actions (profile lookups, contact extraction, one-off messages, connection requests, inbox spot-checks, and invite triage) using the linkedin-cli. Use when the user asks to inspect a LinkedIn member, check unread messages, accept invites, or send an ad-hoc message without running the full background orchestrator.
---

# LinkedIn Quick Actions Skill

This skill provides fast, interactive, and atomic LinkedIn operations powered by the Rust [`linkedin-cli`](../README.md). It is designed for small-scale, conversational actions where the agent works directly with the user in real time, rather than scheduling background jobs through the macro orchestrator.

---

## When to Use This Skill

Use this skill when the user asks to:
1. **Inspect a profile or contact details**: Look up headlines, experience, email addresses, phone numbers, or connection degree for a specific LinkedIn handle.
2. **Review or triage inbound invitations**: Check pending invitations and accept or ignore them with explicit confirmation.
3. **Spot-check the inbox**: View the latest unread threads or read a conversation history with a specific member.
4. **Draft and dispatch a single message or connection request**: Send a targeted 1-on-1 message or connection invite with personalized text approved by the user.
5. **Sample search results**: Run exploratory people searches before configuring a larger campaign.

> [!IMPORTANT]
> **Orchestrator vs. Quick Actions**:
> - Use the **Orchestrator** (`apps/orchestrator`) for continuous background ingestion, large-scale automated campaigns, Huey worker pipelines, and multi-stage review queues.
> - Use this **Skill** for immediate, interactive, single-action human-in-the-loop requests in chat.

---

## Core Guidelines & Safety Protocols

1. **Human-in-the-Loop for Mutations**:
   - For read operations (`profile get`, `inbox list`, `invite list`, `auth status`), execute immediately and display the formatted output.
   - For mutating actions (`message send`, `connection request`, `invite accept`, `invite ignore`, `post create`), **always present the action or drafted text to the user and obtain confirmation before executing**, or run with `-d, --dry-run` first to demonstrate the payload.
2. **Anti-Bot & Rate Limits**:
   - Never run tight loops or mass automation scripts through this skill.
   - Use `--slow-mo <ms>` (e.g. `--slow-mo 500`) if executing consecutive commands.
   - If a CAPTCHA or checkpoint is hit, the CLI tripwire will trigger a system chime and notification for the user to resolve in the browser. Inform the user in chat immediately.
3. **Structured Output**:
   - Use `-o json` (or `--json`) when parsing results with scripts or `jq`.
   - Use default table or markdown formatting when displaying directly to the user.

---

## Workflow Recipes

### 1. Verification & Active Session Status
Before performing actions, ensure the browser session is authenticated:
```bash
# In packages/linkedin-cli:
cargo run --release --bin linkedin -- auth status
# Or if binary is installed on PATH / via mise:
linkedin auth status
```
To inspect who is logged in:
```bash
linkedin auth whoami
```

---

### 2. Member Profile & Contact Intelligence
Fetch structured information on a specific member:
```bash
# Full member overview (headline, about, roles)
linkedin profile get <handle>

# Direct contact info (email, phone, websites)
linkedin profile contact <handle>

# Check connection status (1st, 2nd, 3rd, pending)
linkedin profile status <handle>
```

---

### 3. Inbound Invitations Triage
List pending invitations:
```bash
linkedin invite list --received
```
Once the user identifies who to accept or ignore:
```bash
# Always confirm with user before accepting:
linkedin invite accept <handle>

# Or to decline:
linkedin invite ignore <handle>
```

---

### 4. Inbox Spot-Check & Direct Messaging
Check recent unread conversations:
```bash
linkedin inbox list --unread --limit 5
```

Fetch the full message thread history for a member:
```bash
linkedin message thread <handle> --limit 15
```

Send a direct message (always present draft to user first):
```bash
# 1. Show draft to user in chat:
# "I've drafted the following message for Alice: 'Hi Alice, thanks for connecting!' Shall I send it?"
# 2. Test in dry-run mode:
linkedin message send <handle> --text "..." --dry-run

# 3. Upon user confirmation, execute:
linkedin message send <handle> --text "..."
```

---

### 5. Sending a Connection Request
```bash
# Validate length (LinkedIn notes have a strict 300 character limit):
linkedin connection request <handle> --note "Hi <name>, I came across your work on ... and would love to connect." --dry-run

# Execute upon approval:
linkedin connection request <handle> --note "..."
```

---

### 6. Interactive People Search Sampling
Search people with faceted filters to give the user quick market feedback:
```bash
linkedin search people "<title or keywords>" --location "<city or country>" --network second --page 1
```

---

## Execution Helper Script

When running commands from the root repository, helper tasks can be invoked via `mise` or by targeting `packages/linkedin-cli`:
```bash
cd packages/linkedin-cli && cargo run --release --bin linkedin -- <subcommand> [flags]
```
If a persistent session daemon is desired to speed up sequential commands without closing Chromium:
```bash
linkedin session open --name default --headed
# Subsequent commands attach instantly to the running daemon
linkedin session close --name default
```
