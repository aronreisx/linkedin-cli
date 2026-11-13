---
name: linkedin-quick-actions
description: Fast, targeted LinkedIn actions (profile lookups, contact extraction, one-off messages, connection requests, inbox spot-checks, invite triage, feed interactions, job scouting/saving, and job application outreach) using the linkedin-cli. Use when the user asks to inspect a LinkedIn member, browse/react/comment on feed posts, search/save jobs, apply to jobs with tailored resumes, check unread messages, accept invites, or send an ad-hoc message without running the full background orchestrator.
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
5. **Feed & Timeline interactions**: Read feed updates, inspect post comments, or safely react, comment, or repost with human jitter.
6. **Job scouting & bookmarking**: Search jobs with faceted filters (remote, timeframe, experience), evaluate detailed descriptions, identify hiring managers, and save shortlisted postings.
7. **Job application & recruiter outreach pairing**: Pair job scouting with direct recruiter connection requests, and automate Easy Apply flows with tailored resumes under strict human-in-the-loop review gates.
8. **Sample search results**: Run exploratory people searches before configuring a larger campaign.
9. **Targeted page inspection**: Evaluate arbitrary page elements, dynamic forms, or custom ATS flows using `linkedin debug eval --url <url>`.

> [!IMPORTANT]
> **Orchestrator vs. Quick Actions**:
> - Use the **Orchestrator** (`apps/orchestrator`) for continuous background ingestion, large-scale automated campaigns, Huey worker pipelines, and multi-stage review queues.
> - Use this **Skill** for immediate, interactive, single-action human-in-the-loop requests in chat.

---

## Core Guidelines & Safety Protocols

1. **Human-in-the-Loop for Mutations**:
   - For read operations (`profile get`, `inbox list`, `invite list`, `feed list`, `feed comments`, `jobs search`, `jobs get`, `jobs saved`, `auth status`), execute immediately and display the formatted output.
   - For mutating actions (`message send`, `connection request`, `invite accept`, `invite ignore`, `post create`, `feed react`, `feed comment`, `feed repost`, `jobs save`, `jobs unsave`), **always present the action or drafted text to the user and obtain confirmation before executing**, or run with `-d, --dry-run` first to demonstrate the payload.
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

### 7. Feed & Timeline Engagements
Read posts, inspect discussions, and engage with explicit user approval:
```bash
# Read recent posts from feed (read-only):
linkedin feed list --limit 5

# Fetch posts from a member's activity:
linkedin feed member <handle> --limit 5

# List comments on a post:
linkedin feed comments <urn> --limit 5

# React to a post (always dry-run or verify with user first):
linkedin feed react <urn> --type like --dry-run
linkedin feed react <urn> --type insightful

# Comment on a post (always verify drafted text with user first):
linkedin feed comment <urn> --text "Thanks for sharing this!" --dry-run
linkedin feed comment <urn> --text "Thanks for sharing this!"

# Repost:
linkedin feed repost <urn> --dry-run
linkedin feed repost <urn> --text "Great read!"
```

---

### 8. Job Scouting & Talent Intelligence
Search job opportunities, extract requirements, and shortlist roles:
```bash
# Search remote jobs posted in the past week with Easy Apply:
linkedin jobs search "Staff Rust Engineer" --location "Remote" --workplace remote --posted past-week --easy-apply --limit 10

# Inspect job description, hiring recruiter, salary, and required skills:
linkedin jobs get <job-id> --json

# Save a job to shortlisted items (always confirm with user first):
linkedin jobs save <job-id> --dry-run
linkedin jobs save <job-id>

# View currently saved job postings:
linkedin jobs saved --limit 10
```

---

### 9. Full-Cycle Job Application & Recruiter Outreach Pairing
Pairing job applications with direct recruiter outreach maximizes visibility:
```bash
# 1. Inspect role and hiring team details:
linkedin jobs get <job-id> --json

# 2. Scout recruiters at the hiring company:
linkedin search people "<company> recruiter" --page 1 --json

# 3. Formulate a personalized connection note (< 300 chars):
# "Hi [Name], I just applied to [Company]'s [Role]. With 10+ yrs in backend platforms and deep production experience building agent runtimes, LLM evaluation, and RAG systems, I'd love to connect. Best, [My Name]"

# 4. Dry-run first to verify length and payload:
linkedin connection request <handle> --note "..." --dry-run

# 5. Dispatch upon user approval:
linkedin connection request <handle> --note "..."
```

---

### 10. Dynamic Form & Easy Apply Protocol (Human-in-the-Loop)
When assisting with LinkedIn Easy Apply multi-page applications:
1. **Strict Review Gate Policy**:
   - Inspect and fill contact information, upload resume, and answer questionnaire screening questions.
   - **Never click the final `Submit application` button autonomously.**
   - Stop at the final Review page (`Review your application`), extract all filled fields, and prompt the user for explicit confirmation before submitting.
2. **React Synthetic Event Propagation**:
   - When setting values on inputs or `<select>` controls in LinkedIn/ClearCompany forms, dispatch `input`, `change`, and `blur` events via the element prototype setter (`Object.getOwnPropertyDescriptor(proto, 'value').set.call(el, val)`) so React registers state changes before clicking `Next`.
3. **File Upload Handling**:
   - Clicking "Upload resume" triggers a dynamically created `<input type="file">`.
   - Intercept this file input, populate `input.files` using `DataTransfer` with a base64-decoded PDF blob, and dispatch `change` and `input` events to cleanly upload resumes without OS file dialog dependencies.

---

### 11. Ad-Hoc Page Introspection with `debug eval --url`
When standard CLI commands do not expose a specific interactive element or custom page structure:
```bash
# Inspect page title and application buttons directly:
linkedin debug eval --url "https://www.linkedin.com/jobs/view/<job-id>/" \
  '(() => ({ title: document.title, hasEasyApply: !!document.querySelector("button[aria-label*=\"Easy Apply\"]") }))()'
```
Use read-only introspection to inspect form structures and report them to the user before taking mutating actions.

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
