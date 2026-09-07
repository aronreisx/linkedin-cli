# Incomplete Features & Implementation Reservations

This document highlights CLI subcommands and capabilities within `linkedin-cli-rust` that are currently **incomplete, stubbed, or reliant on mock data**, along with technical reservations and operational notes for developers.

---

## 1. Post Creation (`linkedin post create`)

* **Command**: `linkedin post create --file <path.md> [--media <path>] [--visibility public|connections]`
* **Status**: **Stubbed (Local Processing Only)**
* **What is Implemented**:
  * Markdown file reading and YAML/frontmatter extraction via [`src/domain/content.rs`](file:///Users/aronreis/Projects/linkedin-cli-rust/src/domain/content.rs).
  * UTF-16 character counting and boundary limit validation (3,000 characters maximum).
  * Image/video media upload plan generation.
* **Reservations & Incomplete Gaps**:
  * **No Live Browser Automation**: Does not launch Chromium or navigate to LinkedIn.
  * **Dummy URN Generation**: Returns a simulated identifier (`urn:li:share:<random_u32>`) rather than a real LinkedIn activity URN.
  * **No CDP File Upload Protocol**: Multi-step file upload handshakes against LinkedIn's asset endpoints are not yet wired to live CDP sessions.

---

## 2. Post Analytics (`linkedin post stats`)

* **Command**: `linkedin post stats <activity_urn>`
* **Status**: **Simulated Mock Data**
* **What is Implemented**:
  * Voyager analytics schema parser [`parse_voyager_analytics`](file:///Users/aronreis/Projects/linkedin-cli-rust/src/domain/content.rs).
  * Mathematical metric calculations (CTR, engagement rate, impression counts).
  * Demographic breakdown serialization.
* **Reservations & Incomplete Gaps**:
  * **Static Hardcoded Payload**: Always parses an offline hardcoded sample payload (`impressionCount: 1420`, etc.).
  * **No Network Queries**: Does not query LinkedIn Voyager REST endpoints or inspect live post engagement metrics via CDP.

---

## 3. Terminal UI (`linkedin tui`)

* **Command**: `linkedin tui [--tab invitations|messages|drafts]`
* **Status**: **Interactive UI Shell with Mock Data**
* **What is Implemented**:
  * Ratatui split-pane layout with tabs (`Invitations`, `Messages`, `Drafts`).
  * Keyboard navigation (`1`/`2`/`3`, `j`/`k`, `Up`/`Down`, `q`/`Esc`).
  * Detail pane rendering for selected items.
* **Reservations & Incomplete Gaps**:
  * **Hardcoded Sample Items**: Uses [`create_mock_tui_items`](file:///Users/aronreis/Projects/linkedin-cli-rust/src/infra/tui_infra.rs) ("Dave Recruiter", "Eve Engineer", etc.).
  * **Not Connected to Live Session or Database**: Does not pull live invitations from Chromium, conversations from the messaging service, or staged drafts from `drafts.db`.

---

## 4. Post Draft Execution & Auto-Publishing (`linkedin post draft export`)

* **Command**: `linkedin post draft export --id <id> [--format cron|systemd]`
* **Status**: **Syntax Generator Complete; Automation Runner Missing**
* **What is Implemented**:
  * Local draft persistence in embedded SQLite (`~/.linkedin-cli/drafts.db`).
  * Valid Linux crontab syntax generation.
  * Valid systemd `.service` and `.timer` unit generation.
* **Reservations & Incomplete Gaps**:
  * **Missing Publishing Subcommand**: The exported cron/systemd scripts target `linkedin post publish --draft-id <id>`, which has not yet been implemented in the CLI command hierarchy.
  * **Dependency on Live Post Engine**: Because `linkedin post create` is stubbed, triggering scheduled draft execution will not post live content until browser publishing automation is implemented.

---

## Summary Matrix

| Subcommand | Local Engine / Storage | Live LinkedIn CDP / API | Notes |
| :--- | :---: | :---: | :--- |
| `linkedin post create` | Complete | **Missing** | Emits fake `urn:li:share:...` |
| `linkedin post stats` | Complete | **Missing** | Returns hardcoded sample metrics |
| `linkedin tui` | Complete | **Disconnected** | Operates on static mock data |
| `linkedin post draft <add\|list>` | Complete | N/A (Local SQLite) | Fully functional local staging |
| `linkedin post draft export` | Complete | **Runner Missing** | Targets non-existent `post publish` |
