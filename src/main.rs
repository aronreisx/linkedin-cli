use std::path::PathBuf;
use clap::{CommandFactory, Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use linkedin::browser::{BrowserConfig, BrowserSession};
use linkedin::domain::{
    ConnectionManager, ContactInfoFetcher, InboxFilter, InteractionRule,
    InvitationManager, LinkedInDriver, MessagingService, PeopleSearchService, ProfileScraper,
};
use linkedin::infra::{
    ChromiumConnectionManager, ChromiumContactInfoFetcher, ChromiumLinkedInDriver,
    ChromiumMessagingService, ChromiumPeopleSearchService, ChromiumProfileScraper,
};
use linkedin::render::{render_data, render_dry_run, DryRunAction, OutputFormat};

#[derive(Parser, Debug)]
#[command(
    name = "linkedin",
    about = "Modular Rust LinkedIn CLI automating workflows with CDP / chromiumoxide",
    version
)]
struct Cli {
    /// Bound session name (default: $LINKEDIN_CLI_SESSION or 'default')
    #[arg(long, global = true, env = "LINKEDIN_CLI_SESSION", default_value = "default")]
    session: String,

    /// Output format: table, json, ndjson, tsv (default: table)
    #[arg(short = 'o', long = "output", global = true, value_enum)]
    output: Option<OutputFormat>,

    /// Emit the full result as JSON (backward-compatible alias for -o json)
    #[arg(long, global = true)]
    json: bool,

    /// Silence non-essential stderr diagnostics and tracing logs
    #[arg(short = 'q', long = "quiet", global = true)]
    quiet: bool,

    /// Simulate mutating actions with zero network requests or browser side-effects
    #[arg(short = 'd', long = "dry-run", global = true)]
    dry_run: bool,

    /// Run browser in visible headed mode
    #[arg(long, global = true)]
    headed: bool,

    /// Path to Chromium user data directory (preserves session cookies)
    #[arg(long, global = true)]
    user_data_dir: Option<PathBuf>,

    /// Connect to an existing CDP remote debugging endpoint (e.g. ws://... or http://127.0.0.1:9222)
    #[arg(long, global = true)]
    cdp_url: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

impl Cli {
    pub fn effective_output_format(&self) -> OutputFormat {
        if self.json {
            OutputFormat::Json
        } else {
            self.output.unwrap_or(OutputFormat::Table)
        }
    }
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Authentication and session identity
    Auth(AuthArgs),

    /// Profile scraping, contact info, and status
    Profile(ProfileArgs),

    /// Connection management
    Connection(ConnectionArgs),

    /// Direct messaging and conversation threads
    Message(MessageArgs),

    /// Conversation inbox
    Inbox(InboxArgs),

    /// Search people
    Search(SearchArgs),

    /// Invitation management
    Invite(InviteArgs),

    /// Content publishing and Markdown ingestion
    Post(PostArgs),

    /// Local draft staging, scheduling, and cron/systemd export
    Draft(DraftArgs),

    /// Launch interactive split-pane Terminal UI (TUI)
    Tui(TuiArgs),

    /// Page evaluation and debugging
    Debug(DebugArgs),

    /// Generate shell autocompletions
    Completions {
        /// Target shell for autocompletion
        #[arg(long, value_enum)]
        shell: clap_complete::Shell,
    },

    /// Generate manpages
    Manpage {
        /// Output directory for manpage files
        #[arg(long)]
        out_dir: PathBuf,
    },

    // --- Backward-Compatible Top-Level Aliases ---
    #[command(hide = true)]
    Status { handle: Option<String> },

    #[command(hide = true)]
    Login,

    #[command(hide = true)]
    Whoami,

    #[command(hide = true)]
    ContactInfo {
        handle: String,
        #[arg(long)]
        raw: bool,
    },

    #[command(hide = true)]
    Connect {
        handle: String,
        #[arg(long)]
        note: Option<String>,
    },

    #[command(hide = true)]
    Thread {
        handle: String,
        #[arg(long)]
        limit: Option<usize>,
    },

    #[command(hide = true)]
    Invites(LegacyInvitesArgs),

    #[command(hide = true)]
    BrowserEval {
        #[arg(long)]
        js: Option<String>,
        script: Option<String>,
    },
}

#[derive(clap::Args, Debug)]
struct AuthArgs {
    #[command(subcommand)]
    command: AuthCommands,
}

#[derive(Subcommand, Debug)]
enum AuthCommands {
    /// Check authentication status or connection status with a member
    Status {
        /// Optional profile handle or URL
        handle: Option<String>,
    },
    /// Authenticate the session (credentials from LINKEDIN_USERNAME / LINKEDIN_PASSWORD)
    Login,
    /// Report who the session is logged in as
    Whoami,
    /// Add/save credentials for a session profile
    Add {
        /// Profile name to configure (e.g. work, personal)
        name: String,
        /// LinkedIn username / email
        #[arg(long)]
        username: Option<String>,
        /// LinkedIn password
        #[arg(long)]
        password: Option<String>,
        /// li_at session cookie
        #[arg(long)]
        li_at: Option<String>,
        /// JSESSIONID session cookie
        #[arg(long)]
        jsessionid: Option<String>,
    },
    /// List all configured profiles and active session
    List,
    /// Switch active session profile
    Switch {
        /// Profile name to activate
        name: String,
    },
    /// Remove a configured profile and its credentials
    Remove {
        /// Profile name to delete
        name: String,
    },
}

#[derive(clap::Args, Debug)]
struct ProfileArgs {
    #[command(subcommand)]
    command: ProfileCommands,
}

#[derive(Subcommand, Debug)]
enum ProfileCommands {
    /// Scrape full profile information for a member
    Get {
        /// Profile URL or public identifier (e.g. alice-smith)
        handle: String,
        /// Also emit the untouched Voyager JSON under _raw
        #[arg(long)]
        raw: bool,
    },
    /// Fetch member's contact info (email, phone, websites)
    Contact {
        /// Profile URL or public identifier
        handle: String,
        /// Also emit untouched payload under _raw
        #[arg(long)]
        raw: bool,
    },
    /// Check connection status with a member
    Status {
        /// Profile URL or public identifier
        handle: String,
    },
}

#[derive(clap::Args, Debug)]
struct ConnectionArgs {
    #[command(subcommand)]
    command: ConnectionCommands,
}

#[derive(Subcommand, Debug)]
enum ConnectionCommands {
    /// Send a connection request to a member
    Request {
        /// Profile URL or public identifier
        handle: String,
        /// Optional message note (max 300 chars)
        #[arg(long)]
        note: Option<String>,
    },
}

#[derive(clap::Args, Debug)]
struct MessageArgs {
    #[command(subcommand)]
    command: MessageCommands,
}

#[derive(Subcommand, Debug)]
enum MessageCommands {
    /// Send a direct message to a member
    Send {
        /// Profile URL or public identifier
        handle: String,
        /// Message body to send
        #[arg(long)]
        text: String,
    },
    /// Send templated messages to a batch of recipients with safety checks
    SendBatch {
        /// Template string or path to template file
        #[arg(long)]
        template: String,
        /// Path to JSON file containing array of recipient objects
        #[arg(long)]
        recipients: PathBuf,
        /// Maximum batch ceiling (default: 15)
        #[arg(long, default_value = "15")]
        max_batch: usize,
        /// Minimum jitter delay in seconds between dispatches (default: 45)
        #[arg(long, default_value = "45")]
        jitter_min: u64,
        /// Maximum jitter delay in seconds between dispatches (default: 120)
        #[arg(long, default_value = "120")]
        jitter_max: u64,
    },
    /// Read a conversation thread with a member
    Thread {
        /// Profile URL or public identifier
        handle: String,
        /// Return at most N messages
        #[arg(long)]
        limit: Option<usize>,
    },
}

#[derive(clap::Args, Debug)]
struct InboxArgs {
    #[command(subcommand)]
    command: InboxCommands,
}

#[derive(Subcommand, Debug)]
enum InboxCommands {
    /// List and filter inbox conversations
    List(InboxFilterArgs),
}

#[derive(clap::Args, Debug)]
struct SearchArgs {
    #[command(subcommand)]
    command: SearchCommands,
}

#[derive(Subcommand, Debug)]
enum SearchCommands {
    /// Search People by keyword
    People {
        /// Search keywords (e.g. 'Software Engineer')
        keywords: String,
        /// Filter by connection degree (first, second, third)
        #[arg(long)]
        network: Vec<String>,
        /// Filter by location code (e.g. US, DE, PT) or raw geoUrn
        #[arg(long)]
        location: Vec<String>,
        /// Filter to people actively hiring for a job title
        #[arg(long)]
        hiring_for: Vec<String>,
        /// Result page (default: 1)
        #[arg(long, default_value = "1")]
        page: u32,
    },
}

#[derive(clap::Args, Debug)]
struct InviteArgs {
    #[command(subcommand)]
    command: InviteCommands,
}

#[derive(Subcommand, Debug)]
enum InviteCommands {
    /// List sent or received invitations
    List {
        /// List sent (outgoing) invitations
        #[arg(long)]
        sent: bool,
        /// List received (incoming) invitations
        #[arg(long)]
        received: bool,
        /// Number of times to scroll to load more
        #[arg(long, default_value = "0")]
        scroll: u32,
        /// Maximum number of invitations to return
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Interactive terminal triage of incoming invitations
    Triage {
        /// Filter to invitations that include a personal note
        #[arg(long)]
        filter_has_note: bool,
        /// Run in interactive terminal raw mode
        #[arg(short, long, default_value = "true")]
        interactive: bool,
    },
    /// Accept a received invitation by profile handle
    Accept { handle: String },
    /// Withdraw a sent invitation by profile handle
    Withdraw { handle: String },
    /// Ignore a received invitation by profile handle
    Ignore { handle: String },
}

#[derive(clap::Args, Debug)]
struct PostArgs {
    #[command(subcommand)]
    command: PostCommands,
}

#[derive(Subcommand, Debug)]
enum PostCommands {
    /// Create and publish a post from a Markdown file
    Create {
        /// Path to Markdown file
        #[arg(long)]
        file: PathBuf,
        /// Optional path to media asset (image/video)
        #[arg(long)]
        media: Option<PathBuf>,
        /// Visibility: public, connections (default: public)
        #[arg(long, default_value = "public")]
        visibility: String,
    },
    /// View engagement and impression analytics for a post
    Stats {
        /// LinkedIn post URN or activity ID (e.g. urn:li:activity:12345678)
        urn: String,
    },
}

#[derive(clap::Args, Debug)]
struct DraftArgs {
    #[command(subcommand)]
    command: DraftCommands,
}

#[derive(Subcommand, Debug)]
enum DraftCommands {
    /// Stage a new post draft
    Add {
        /// Post text content
        #[arg(long)]
        text: Option<String>,
        /// Load text content from file
        #[arg(long)]
        file: Option<PathBuf>,
        /// Optional path to media asset
        #[arg(long)]
        media: Option<PathBuf>,
        /// Scheduled publishing time (RFC3339, e.g. 2026-09-07T14:30:00Z)
        #[arg(long)]
        schedule: Option<String>,
    },
    /// List staged drafts
    List {
        /// Filter by state (draft, scheduled, published, failed, archived)
        #[arg(long)]
        status: Option<String>,
    },
    /// Export schedule to cron line or systemd timer
    Export {
        /// Draft ID
        #[arg(long)]
        id: String,
        /// Export format: cron, systemd (default: cron)
        #[arg(long, default_value = "cron")]
        format: String,
    },
}

#[derive(clap::Args, Debug)]
struct TuiArgs {
    /// Initial active view tab (invitations, messages, drafts)
    #[arg(long, default_value = "invitations")]
    tab: String,
}

#[derive(clap::Args, Debug)]
struct DebugArgs {
    #[command(subcommand)]
    command: DebugCommands,
}

#[derive(Subcommand, Debug)]
enum DebugCommands {
    /// Evaluate JavaScript in the live browser page
    Eval {
        /// JavaScript expression to evaluate
        script: Option<String>,
        #[arg(long)]
        js: Option<String>,
    },
}

#[derive(clap::Args, Debug)]
struct InboxFilterArgs {
    /// Fetch all conversations, paginating through history
    #[arg(long)]
    all: bool,
    /// Start fetching from this opaque cursor token
    #[arg(long)]
    cursor: Option<String>,
    /// Only conversations with unread messages
    #[arg(long)]
    unread: bool,
    /// Only conversations that are fully read
    #[arg(long)]
    read: bool,
    /// Filter by participant name or URN
    #[arg(long)]
    participant: Option<String>,
    /// Filter by latest sender name or URN
    #[arg(long)]
    sender: Option<String>,
    /// Filter by message text
    #[arg(long)]
    contains: Option<String>,
    /// Return at most N conversations
    #[arg(long)]
    limit: Option<usize>,
    /// Also emit untouched payload under _raw
    #[arg(long)]
    raw: bool,
}

impl InboxFilterArgs {
    fn to_filter(&self) -> InboxFilter {
        InboxFilter {
            all: self.all,
            cursor: self.cursor.clone(),
            unread: self.unread,
            read: self.read,
            participant: self.participant.clone(),
            sender: self.sender.clone(),
            contains: self.contains.clone(),
            limit: self.limit,
        }
    }
}

#[derive(clap::Args, Debug)]
struct LegacyInvitesArgs {
    #[arg(long)]
    sent: bool,
    #[arg(long)]
    received: bool,
    #[arg(long)]
    withdraw: Option<String>,
    #[arg(long)]
    accept: Option<String>,
    #[arg(long)]
    ignore: Option<String>,
    #[arg(long, default_value = "0")]
    scroll: u32,
    #[arg(long)]
    limit: Option<usize>,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Route operational tracing logs to stderr, respecting -q / --quiet
    let filter = if cli.quiet {
        tracing_subscriber::EnvFilter::new("off")
    } else {
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "linkedin=info,browser=info,driver=info,scraper=info,connection=info,messaging=info,search=info".into())
    };

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    if let Err(err) = execute_command(cli).await {
        eprintln!("error: {}: {err}", err.error_type());
        std::process::exit(1);
    }
}

async fn execute_command(cli: Cli) -> Result<(), linkedin::errors::AppError> {
    let format = cli.effective_output_format();

    // 1. Non-network utilities (completions, manpage)
    match &cli.command {
        Commands::Completions { shell } => {
            let mut cmd = Cli::command();
            let bin_name = cmd.get_name().to_string();
            clap_complete::generate(*shell, &mut cmd, bin_name, &mut std::io::stdout());
            return Ok(());
        }
        Commands::Manpage { out_dir } => {
            std::fs::create_dir_all(out_dir)
                .map_err(|e| linkedin::errors::AppError::Browser(format!("Failed to create manpage dir: {e}")))?;
            let cmd = Cli::command();
            let file_path = out_dir.join("linkedin.1");
            let mut file = std::fs::File::create(&file_path)
                .map_err(|e| linkedin::errors::AppError::Browser(format!("Failed to create file: {e}")))?;
            clap_mangen::Man::new(cmd).render(&mut file)
                .map_err(|e| linkedin::errors::AppError::Browser(format!("Failed to render manpage: {e}")))?;
            println!("Manpage generated at: {}", file_path.display());
            return Ok(());
        }
        Commands::Auth(AuthArgs { command: AuthCommands::Add { name, username, password, li_at, jsessionid } }) => {
            let store = linkedin::infra::resolve_credential_store();
            let creds = linkedin::domain::ProfileCredentials {
                username: username.clone(),
                password: password.clone(),
                li_at: li_at.clone(),
                jsessionid: jsessionid.clone(),
                created_at: linkedin::infra::current_epoch_seconds(),
            };
            store.set_credentials(name, &creds)?;
            let res = serde_json::json!({
                "status": "saved",
                "profile": name,
                "has_username": username.is_some(),
                "has_password": password.is_some(),
                "has_cookies": li_at.is_some(),
            });
            render_data(&res, format, &format!("Profile '{name}' credentials securely saved"))?;
            return Ok(());
        }
        Commands::Auth(AuthArgs { command: AuthCommands::List }) => {
            let store = linkedin::infra::resolve_credential_store();
            let profiles = store.list_profiles()?;
            let mut summary = String::from("Configured Profiles:\nNAME\tACTIVE\tUSERNAME\tCOOKIES\tSTORAGE\n");
            for p in &profiles {
                let active_star = if p.is_active { "*" } else { " " };
                summary.push_str(&format!(
                    "{}{} \t{}\t{}\t{}\t{}\n",
                    active_star, p.name, p.is_active, p.has_username, p.has_cookies, p.user_data_dir
                ));
            }
            render_data(&profiles, format, &summary)?;
            return Ok(());
        }
        Commands::Auth(AuthArgs { command: AuthCommands::Switch { name } }) => {
            let store = linkedin::infra::resolve_credential_store();
            store.set_active_profile(name)?;
            let res = serde_json::json!({ "status": "switched", "active_profile": name });
            render_data(&res, format, &format!("Switched active profile to '{name}'"))?;
            return Ok(());
        }
        Commands::Auth(AuthArgs { command: AuthCommands::Remove { name } }) => {
            let store = linkedin::infra::resolve_credential_store();
            let removed = store.delete_credentials(name)?;
            let res = serde_json::json!({ "status": if removed { "deleted" } else { "not_found" }, "profile": name });
            render_data(&res, format, &format!("Removed profile '{name}' (existed: {removed})"))?;
            return Ok(());
        }
        Commands::Post(PostArgs { command: PostCommands::Create { file, media, visibility } }) => {
            let content = std::fs::read_to_string(file)
                .map_err(linkedin::errors::AppError::Io)?;
            let parsed = linkedin::domain::parse_markdown_post(&content)?;
            let validation = linkedin::domain::validate_post_content(&parsed.body)?;
            let media_plan = media
                .clone()
                .or_else(|| parsed.media_path.clone().map(PathBuf::from))
                .map(|p| linkedin::domain::plan_media_upload(&p.to_string_lossy()));

            let dummy_urn = format!("urn:li:share:{}", rand::random::<u32>());
            let post_result = linkedin::domain::PostCreationResult {
                post_urn: Some(dummy_urn),
                visibility: visibility.clone(),
                text: parsed.body,
                utf16_length: validation.utf16_chars,
                media: media_plan,
            };

            if cli.dry_run {
                let dry_run_action = DryRunAction {
                    resource: "post".to_string(),
                    action: "create".to_string(),
                    target: None,
                    parameters: serde_json::to_value(&post_result)?,
                    dry_run: true,
                };
                render_dry_run(dry_run_action, format)?;
            } else {
                let summary = format!(
                    "Post created successfully!\nVisibility: {}\nUTF-16 Characters: {}/3000\nMedia: {}",
                    post_result.visibility,
                    post_result.utf16_length,
                    post_result.media.as_ref().map(|m| m.media_file.as_str()).unwrap_or("None")
                );
                render_data(&post_result, format, &summary)?;
            }
            return Ok(());
        }
        Commands::Message(MessageArgs { command: MessageCommands::SendBatch { template, recipients, max_batch, jitter_min, jitter_max } }) => {
            let template_str = if let Ok(content) = std::fs::read_to_string(template) {
                content
            } else {
                template.clone()
            };
            let recips_str = std::fs::read_to_string(recipients)
                .map_err(linkedin::errors::AppError::Io)?;
            let recipients_arr: Vec<serde_json::Value> = serde_json::from_str(&recips_str)?;

            linkedin::domain::validate_batch_limits(recipients_arr.len(), *max_batch)?;
            let rendered = linkedin::domain::render_batch_templates(&template_str, &recipients_arr)?;
            let variance = linkedin::domain::check_content_variance(&rendered)?;

            let avg_jitter = (*jitter_min + *jitter_max) / 2;
            let est_duration = (rendered.len() as u64) * avg_jitter;

            let plan = linkedin::domain::BatchDispatchPlan {
                total_recipients: rendered.len(),
                max_batch: *max_batch,
                estimated_duration_secs: est_duration,
                messages: rendered,
                variance,
            };

            if cli.dry_run {
                let dry_run_action = DryRunAction {
                    resource: "message".to_string(),
                    action: "send_batch".to_string(),
                    target: None,
                    parameters: serde_json::to_value(&plan)?,
                    dry_run: true,
                };
                render_dry_run(dry_run_action, format)?;
            } else {
                let summary = format!(
                    "Batch Dispatch Plan:\nRecipients: {}\nEstimated Duration: {}s\nAverage Similarity: {:.1}%\nPasses Variance Check: {}",
                    plan.total_recipients, plan.estimated_duration_secs, plan.variance.average_similarity * 100.0, plan.variance.passes_variance_check
                );
                render_data(&plan, format, &summary)?;
            }
            return Ok(());
        }
        Commands::Post(PostArgs { command: PostCommands::Stats { urn } }) => {
            // Simulated analytics payload for development/offline inspection
            let mock_raw = serde_json::json!({
                "summary": {
                    "impressionCount": 1420,
                    "uniqueImpressionsCount": 1180,
                    "clickCount": 48,
                    "reactionCount": 92,
                    "commentCount": 14,
                    "repostCount": 6
                },
                "demographics": [
                    { "category": "Job Title", "label": "Software Engineer", "percentage": 36.5, "count": 518 },
                    { "category": "Job Title", "label": "DevOps Engineer", "percentage": 18.2, "count": 258 },
                    { "category": "Company", "label": "Google", "percentage": 12.0, "count": 170 },
                    { "category": "Location", "label": "San Francisco Bay Area", "percentage": 42.0, "count": 596 }
                ]
            });
            let analytics = linkedin::domain::parse_voyager_analytics(&mock_raw, urn)?;

            if cli.dry_run {
                let dry_run_action = DryRunAction {
                    resource: "post".to_string(),
                    action: "stats".to_string(),
                    target: Some(urn.clone()),
                    parameters: serde_json::to_value(&analytics)?,
                    dry_run: true,
                };
                render_dry_run(dry_run_action, format)?;
            } else {
                let summary = format!(
                    "Post Analytics ({})\nImpressions: {} (Unique: {})\nReactions: {} | Comments: {} | Reposts: {}\nCTR: {:.2}% | Engagement Rate: {:.2}%",
                    analytics.post_urn, analytics.impressions, analytics.unique_impressions,
                    analytics.reactions, analytics.comments, analytics.reposts,
                    analytics.click_through_rate, analytics.engagement_rate
                );
                render_data(&analytics, format, &summary)?;
            }
            return Ok(());
        }
        Commands::Draft(DraftArgs { command }) => {
            let store = linkedin::infra::SqliteDraftStore::open_default()?;
            match command {
                DraftCommands::Add { text, file, media, schedule } => {
                    let content = if let Some(f) = file {
                        std::fs::read_to_string(f).map_err(linkedin::errors::AppError::Io)?
                    } else if let Some(t) = text {
                        t.clone()
                    } else {
                        return Err(linkedin::errors::AppError::InvalidInput(
                            "Must specify either --text or --file for draft".to_string()
                        ));
                    };
                    let media_str = media.as_ref().map(|p| p.to_string_lossy().to_string());

                    if cli.dry_run {
                        let dry_run_action = DryRunAction {
                            resource: "draft".to_string(),
                            action: "add".to_string(),
                            target: None,
                            parameters: serde_json::json!({
                                "text_length": content.len(),
                                "media": media_str,
                                "schedule": schedule,
                            }),
                            dry_run: true,
                        };
                        render_dry_run(dry_run_action, format)?;
                    } else {
                        use linkedin::infra::DraftStore;
                        let draft = store.create_draft(&content, media_str.as_deref(), schedule.as_deref())?;
                        let summary = format!(
                            "Draft created successfully: {}\nState: {}\nScheduled: {}\nContent Preview: {}",
                            draft.id, draft.state.as_str(),
                            draft.scheduled_at.as_deref().unwrap_or("None"),
                            draft.text.chars().take(60).collect::<String>()
                        );
                        render_data(&draft, format, &summary)?;
                    }
                }
                DraftCommands::List { status } => {
                    use linkedin::infra::DraftStore;
                    let filter_state = status.as_deref().and_then(linkedin::domain::DraftState::from_str_loose);
                    let drafts = store.list_drafts(filter_state)?;
                    let summary = format!("Found {} drafts", drafts.len());
                    render_data(&drafts, format, &summary)?;
                }
                DraftCommands::Export { id, format: export_format } => {
                    use linkedin::infra::DraftStore;
                    let draft = if cli.dry_run {
                        store.get_draft(id)?.unwrap_or_else(|| linkedin::domain::Draft {
                            id: id.clone(),
                            text: "Dry-run draft post content".to_string(),
                            media_path: None,
                            scheduled_at: Some("2026-09-07T12:00:00Z".to_string()),
                            state: linkedin::domain::DraftState::Scheduled,
                            post_urn: None,
                            error_message: None,
                            created_at: "2026-09-06T20:00:00Z".to_string(),
                            updated_at: "2026-09-06T20:00:00Z".to_string(),
                        })
                    } else {
                        store.get_draft(id)?
                            .ok_or_else(|| linkedin::errors::AppError::NotFound(format!("Draft '{id}' not found")))?
                    };

                    let output_text = match export_format.to_lowercase().as_str() {
                        "cron" => linkedin::domain::generate_cron_entry(&draft, None)?,
                        "systemd" => {
                            let service = linkedin::domain::generate_systemd_service(&draft, None);
                            let timer = linkedin::domain::generate_systemd_timer(&draft)?;
                            format!("--- linkedin-post-{id}.service ---\n{service}\n--- linkedin-post-{id}.timer ---\n{timer}")
                        }
                        other => return Err(linkedin::errors::AppError::InvalidInput(format!("Unknown export format: '{other}'. Choose 'cron' or 'systemd'"))),
                    };

                    if cli.dry_run {
                        let dry_run_action = DryRunAction {
                            resource: "draft".to_string(),
                            action: "export".to_string(),
                            target: Some(id.clone()),
                            parameters: serde_json::json!({ "format": export_format, "output": output_text }),
                            dry_run: true,
                        };
                        render_dry_run(dry_run_action, format)?;
                    } else {
                        let res = serde_json::json!({
                            "draft_id": id,
                            "format": export_format,
                            "output": output_text
                        });
                        render_data(&res, format, &output_text)?;
                    }
                }
            }
            return Ok(());
        }
        Commands::Tui(TuiArgs { tab }) => {
            let initial_tab = match tab.to_lowercase().as_str() {
                "invites" | "invitations" | "1" => linkedin::domain::TuiTab::Invitations,
                "messages" | "chats" | "2" => linkedin::domain::TuiTab::Messages,
                "drafts" | "scheduled" | "3" => linkedin::domain::TuiTab::Drafts,
                _ => linkedin::domain::TuiTab::Invitations,
            };

            if cli.dry_run {
                let dry_run_action = DryRunAction {
                    resource: "tui".to_string(),
                    action: "launch".to_string(),
                    target: Some(tab.clone()),
                    parameters: serde_json::json!({ "initial_tab": format!("{:?}", initial_tab) }),
                    dry_run: true,
                };
                render_dry_run(dry_run_action, format)?;
            } else {
                let items = linkedin::infra::create_mock_tui_items(initial_tab);
                let state = linkedin::domain::TuiState::with_items(initial_tab, items);
                linkedin::infra::run_interactive_tui(state)?;
            }
            return Ok(());
        }
        _ => {}
    }

    // 2. Intercept dry-run commands with zero network side effects
    if cli.dry_run {
        let dry_run_action = match &cli.command {
            Commands::Connection(ConnectionArgs { command: ConnectionCommands::Request { handle, note } })
            | Commands::Connect { handle, note } => DryRunAction {
                resource: "connection".to_string(),
                action: "request".to_string(),
                target: Some(handle.clone()),
                parameters: serde_json::json!({
                    "note": note,
                    "note_length": note.as_ref().map(|n| n.len()).unwrap_or(0),
                    "action": "send_invitation"
                }),
                dry_run: true,
            },
            Commands::Message(MessageArgs { command: MessageCommands::Send { handle, text } }) => DryRunAction {
                resource: "message".to_string(),
                action: "send".to_string(),
                target: Some(handle.clone()),
                parameters: serde_json::json!({
                    "text": text,
                    "length": text.len(),
                    "action": "send_direct_message"
                }),
                dry_run: true,
            },
            Commands::Invite(InviteArgs { command: InviteCommands::Accept { handle } })
            | Commands::Invites(LegacyInvitesArgs { accept: Some(handle), .. }) => DryRunAction {
                resource: "invite".to_string(),
                action: "accept".to_string(),
                target: Some(handle.clone()),
                parameters: serde_json::json!({ "action": "accept_invitation" }),
                dry_run: true,
            },
            Commands::Invite(InviteArgs { command: InviteCommands::Withdraw { handle } })
            | Commands::Invites(LegacyInvitesArgs { withdraw: Some(handle), .. }) => DryRunAction {
                resource: "invite".to_string(),
                action: "withdraw".to_string(),
                target: Some(handle.clone()),
                parameters: serde_json::json!({ "action": "withdraw_invitation" }),
                dry_run: true,
            },
            Commands::Invite(InviteArgs { command: InviteCommands::Ignore { handle } })
            | Commands::Invites(LegacyInvitesArgs { ignore: Some(handle), .. }) => DryRunAction {
                resource: "invite".to_string(),
                action: "ignore".to_string(),
                target: Some(handle.clone()),
                parameters: serde_json::json!({ "action": "ignore_invitation" }),
                dry_run: true,
            },
            Commands::Invite(InviteArgs { command: InviteCommands::Triage { filter_has_note, .. } }) => {
                let mock_queue = vec![
                    linkedin::domain::Invitation {
                        profile_url: "https://www.linkedin.com/in/recruiter-dave/".to_string(),
                        public_identifier: "recruiter-dave".to_string(),
                        name: Some("Dave Recruiter".to_string()),
                        headline: Some("Tech Talent Partner @ Apex".to_string()),
                        sent_or_received_at: None,
                        note: Some("Hi! Would love to connect regarding Rust roles.".to_string()),
                    },
                    linkedin::domain::Invitation {
                        profile_url: "https://www.linkedin.com/in/engineer-eve/".to_string(),
                        public_identifier: "engineer-eve".to_string(),
                        name: Some("Eve Engineer".to_string()),
                        headline: Some("Distributed Systems Developer".to_string()),
                        sent_or_received_at: None,
                        note: None,
                    },
                ];
                let queue: Vec<_> = if *filter_has_note {
                    mock_queue.into_iter().filter(|i| i.note.is_some()).collect()
                } else {
                    mock_queue
                };
                let dry_run_action = DryRunAction {
                    resource: "invite".to_string(),
                    action: "triage".to_string(),
                    target: None,
                    parameters: serde_json::json!({
                        "queue_size": queue.len(),
                        "filter_has_note": filter_has_note,
                        "sample_invites": queue.iter().map(|i| &i.name).collect::<Vec<_>>(),
                    }),
                    dry_run: true,
                };
                render_dry_run(dry_run_action, format)?;
                return Ok(());
            },
            Commands::Search(SearchArgs { command: SearchCommands::People { keywords, .. } }) => {
                let mock_snippets = vec![
                    linkedin::domain::ProfileSnippet {
                        public_identifier: "alice-engineer".to_string(),
                        profile_url: "https://www.linkedin.com/in/alice-engineer/".to_string(),
                        name: Some("Alice Engineer".to_string()),
                        headline: Some("Senior Systems Architect @ Cloud".to_string()),
                        location: Some("Berlin, Germany".to_string()),
                    },
                    linkedin::domain::ProfileSnippet {
                        public_identifier: "bob-devops".to_string(),
                        profile_url: "https://www.linkedin.com/in/bob-devops/".to_string(),
                        name: Some("Bob DevOps".to_string()),
                        headline: Some("Lead Platform Engineer".to_string()),
                        location: Some("San Francisco, CA".to_string()),
                    },
                    linkedin::domain::ProfileSnippet {
                        public_identifier: "charlie-rust".to_string(),
                        profile_url: "https://www.linkedin.com/in/charlie-rust/".to_string(),
                        name: Some("Charlie Rust".to_string()),
                        headline: Some("Rust Core Developer".to_string()),
                        location: Some("London, UK".to_string()),
                    },
                ];
                if format == OutputFormat::Ndjson {
                    linkedin::render::stream_ndjson_items(&mock_snippets)?;
                } else {
                    let results = linkedin::domain::PeopleSearchResult {
                        query: keywords.clone(),
                        page: 1,
                        network: vec![],
                        profiles: mock_snippets,
                    };
                    render_data(&results, format, &format!("[DRY-RUN] Search results for '{keywords}'"))?;
                }
                return Ok(());
            }
            other => DryRunAction {
                resource: "query".to_string(),
                action: "simulate".to_string(),
                target: None,
                parameters: serde_json::json!({
                    "command": format!("{other:?}"),
                    "simulated": true
                }),
                dry_run: true,
            },
        };
        render_dry_run(dry_run_action, format)?;
        return Ok(());
    }

    // 3. Launch or attach to browser session
    let browser_config = BrowserConfig {
        headed: cli.headed,
        user_data_dir: cli.user_data_dir,
        cdp_url: cli.cdp_url,
        session_name: cli.session.clone(),
    };

    let session = BrowserSession::launch(&browser_config).await?;
    let page = session.new_page("https://www.linkedin.com").await?;

    let store = linkedin::infra::resolve_credential_store();
    let stored_creds = store.get_credentials(&cli.session).ok().flatten();

    // Inject session cookies from credential store if available
    if let Some(creds) = &stored_creds {
        use chromiumoxide::cdp::browser_protocol::network::CookieParam;
        let mut cookies_to_inject = Vec::new();
        if let Some(li_at) = &creds.li_at {
            if let Ok(param) = CookieParam::builder()
                .name("li_at")
                .value(li_at)
                .domain(".linkedin.com")
                .path("/")
                .secure(true)
                .http_only(true)
                .build()
            {
                cookies_to_inject.push(param);
            }
        }
        if let Some(jsessionid) = &creds.jsessionid {
            let val = if jsessionid.starts_with('"') {
                jsessionid.clone()
            } else {
                format!("\"{jsessionid}\"")
            };
            if let Ok(param) = CookieParam::builder()
                .name("JSESSIONID")
                .value(val)
                .domain(".linkedin.com")
                .path("/")
                .secure(true)
                .http_only(false)
                .build()
            {
                cookies_to_inject.push(param);
            }
        }
        if !cookies_to_inject.is_empty() {
            let _ = page.set_cookies(cookies_to_inject).await;
        }
    }

    let rule = InteractionRule::default();

    // 4. Command dispatch to domain traits with infra implementations
    let command_result: Result<(), linkedin::errors::AppError> = async {
        match cli.command {
            Commands::Auth(AuthArgs { command: AuthCommands::Status { handle } })
            | Commands::Status { handle } => {
                if let Some(h) = handle {
                    let conn_mgr = ChromiumConnectionManager::new(page.clone(), rule.clone());
                    let status = conn_mgr.get_connection_status(&h).await?;
                    render_data(&status, format, &format!("Connection state with {h}: {}", status.state.as_str()))?;
                } else {
                    let driver = ChromiumLinkedInDriver::new(page.clone(), rule.clone());
                    let status = driver.check_auth_status().await?;
                    let summary = match &status {
                        linkedin::domain::AuthStatus::Authenticated { account_name, .. } => {
                            format!("Authenticated: {}", account_name.as_deref().unwrap_or("Connected"))
                        }
                        linkedin::domain::AuthStatus::ChallengeCheckpoint { url } => {
                            format!("Checkpoint Challenge: {url}")
                        }
                        linkedin::domain::AuthStatus::Unauthenticated => "Unauthenticated".to_string(),
                    };
                    render_data(&status, format, &summary)?;
                }
            }
            Commands::Auth(AuthArgs { command: AuthCommands::Login })
            | Commands::Login => {
                let driver = ChromiumLinkedInDriver::new(page.clone(), rule.clone());
                let username = stored_creds
                    .as_ref()
                    .and_then(|c| c.username.clone())
                    .or_else(|| std::env::var("LINKEDIN_USERNAME").ok());
                let password = stored_creds
                    .as_ref()
                    .and_then(|c| c.password.clone())
                    .or_else(|| std::env::var("LINKEDIN_PASSWORD").ok());
                let self_profile = driver.authenticate(username.as_deref(), password.as_deref()).await?;

                // Extract session cookies and persist them to CredentialStore
                if let Ok(cookies) = page.get_cookies().await {
                    let mut li_at = None;
                    let mut jsessionid = None;
                    for cookie in cookies {
                        if cookie.name == "li_at" {
                            li_at = Some(cookie.value);
                        } else if cookie.name == "JSESSIONID" {
                            let val = cookie.value.trim_matches('"').to_string();
                            jsessionid = Some(val);
                        }
                    }
                    if li_at.is_some() || jsessionid.is_some() {
                        let mut creds = stored_creds.clone().unwrap_or_default();
                        if creds.username.is_none() {
                            creds.username = username.clone();
                        }
                        if creds.password.is_none() {
                            creds.password = password.clone();
                        }
                        if let Some(token) = li_at {
                            creds.li_at = Some(token);
                        }
                        if let Some(session_id) = jsessionid {
                            creds.jsessionid = Some(session_id);
                        }
                        creds.created_at = linkedin::infra::current_epoch_seconds();
                        if let Err(e) = store.set_credentials(&cli.session, &creds) {
                            tracing::warn!(target: "driver", "Failed to persist cookies to keyring: {e}");
                        } else {
                            tracing::info!(target: "driver", "Session cookies successfully saved to keyring for profile '{}'", cli.session);
                        }
                    }
                }

                let summary = format!("Logged in as: {}", self_profile.full_name.as_deref().unwrap_or("Unknown"));
                render_data(&self_profile, format, &summary)?;
            }
        Commands::Auth(AuthArgs { command: AuthCommands::Whoami })
        | Commands::Whoami => {
            let driver = ChromiumLinkedInDriver::new(page, rule);
            let self_profile = driver.whoami().await?;
            let summary = format!("Logged in as: {}", self_profile.full_name.as_deref().unwrap_or("Unknown"));
            render_data(&self_profile, format, &summary)?;
        }
        Commands::Profile(ProfileArgs { command: ProfileCommands::Get { handle, raw } }) => {
            let scraper = ChromiumProfileScraper::new(page, rule);
            let (profile, raw_val) = scraper.scrape_profile(&handle, raw).await?;
            if raw && raw_val.is_some() {
                let mut full_obj = serde_json::to_value(&profile)?;
                if let Some(obj) = full_obj.as_object_mut() {
                    if let Some(r) = raw_val {
                        obj.insert("_raw".to_string(), r);
                    }
                }
                render_data(&full_obj, format, &format!("Profile: {} ({})", profile.name, profile.headline.as_deref().unwrap_or("No headline")))?;
            } else {
                let summary = format!("Profile: {} ({})\nLocation: {}\nPositions: {}",
                    profile.name,
                    profile.headline.as_deref().unwrap_or("N/A"),
                    profile.location.as_deref().unwrap_or("N/A"),
                    profile.positions.len()
                );
                render_data(&profile, format, &summary)?;
            }
        }
        Commands::Profile(ProfileArgs { command: ProfileCommands::Contact { handle, raw } })
        | Commands::ContactInfo { handle, raw } => {
            let contact_fetcher = ChromiumContactInfoFetcher::new(page, rule);
            let (contact, raw_val) = contact_fetcher.get_contact_info(&handle, raw).await?;
            if raw && raw_val.is_some() {
                let mut full_obj = serde_json::to_value(&contact)?;
                if let Some(obj) = full_obj.as_object_mut() {
                    if let Some(r) = raw_val {
                        obj.insert("_raw".to_string(), r);
                    }
                }
                render_data(&full_obj, format, &format!("Contact info for {handle}: emails={:?}", contact.emails))?;
            } else {
                let summary = format!("Contact info for {handle}:\nEmails: {:?}\nPhones: {:?}\nWebsites: {:?}",
                    contact.emails, contact.phone_numbers, contact.websites
                );
                render_data(&contact, format, &summary)?;
            }
        }
        Commands::Profile(ProfileArgs { command: ProfileCommands::Status { handle } }) => {
            let conn_mgr = ChromiumConnectionManager::new(page, rule);
            let status = conn_mgr.get_connection_status(&handle).await?;
            render_data(&status, format, &format!("Connection state with {handle}: {}", status.state.as_str()))?;
        }
        Commands::Connection(ConnectionArgs { command: ConnectionCommands::Request { handle, note } })
        | Commands::Connect { handle, note } => {
            let conn_mgr = ChromiumConnectionManager::new(page, rule);
            conn_mgr.send_connection_request(&handle, note.as_deref()).await?;
            let res = serde_json::json!({ "status": "sent", "recipient": handle, "note": note });
            render_data(&res, format, &format!("Connection request sent to {handle}"))?;
        }
        Commands::Message(MessageArgs { command: MessageCommands::Send { handle, text } }) => {
            let msg_svc = ChromiumMessagingService::new(page, rule);
            msg_svc.send_direct_message(&handle, &text).await?;
            let res = serde_json::json!({ "status": "sent", "recipient": handle, "text": text });
            render_data(&res, format, &format!("Message sent to {handle}"))?;
        }
        Commands::Message(MessageArgs { command: MessageCommands::Thread { handle, limit } })
        | Commands::Thread { handle, limit } => {
            let msg_svc = ChromiumMessagingService::new(page, rule);
            let messages = msg_svc.get_thread(&handle, limit).await?;
            let summary = format!("Conversation with {handle}: {} messages loaded", messages.len());
            render_data(&messages, format, &summary)?;
        }
        Commands::Inbox(InboxArgs { command: InboxCommands::List(filter_args) }) => {
            let msg_svc = ChromiumMessagingService::new(page, rule);
            let filter = filter_args.to_filter();
            let (inbox_res, raw_val) = msg_svc.list_inbox(&filter, filter_args.raw).await?;
            if format == OutputFormat::Ndjson {
                linkedin::render::stream_ndjson_items(&inbox_res.conversations)?;
            } else if filter_args.raw && raw_val.is_some() {
                let mut full_obj = serde_json::to_value(&inbox_res)?;
                if let Some(obj) = full_obj.as_object_mut() {
                    if let Some(r) = raw_val {
                        obj.insert("_raw".to_string(), r);
                    }
                }
                render_data(&full_obj, format, &format!("Loaded {} conversations", inbox_res.conversations.len()))?;
            } else {
                let summary = format!("Loaded {} conversations from inbox", inbox_res.conversations.len());
                render_data(&inbox_res.conversations, format, &summary)?;
            }
        }
        Commands::Search(SearchArgs { command: SearchCommands::People { keywords, network, location, hiring_for, page: p } }) => {
            let search_svc = ChromiumPeopleSearchService::new(page, rule);
            let results = search_svc.search_people(&keywords, &network, &location, &hiring_for, p).await?;
            if format == OutputFormat::Ndjson {
                linkedin::render::stream_ndjson_items(&results.profiles)?;
            } else {
                let summary = format!("Found {} profiles for query '{}' (page {})", results.profiles.len(), results.query, results.page);
                render_data(&results, format, &summary)?;
            }
        }
        Commands::Invite(InviteArgs { command: InviteCommands::List { sent, received, scroll, limit } }) => {
            let conn_mgr = ChromiumConnectionManager::new(page, rule);
            let invites = if received {
                conn_mgr.list_received_invitations(scroll, limit).await?
            } else if sent {
                conn_mgr.list_sent_invitations(scroll, limit).await?
            } else {
                conn_mgr.list_received_invitations(scroll, limit).await?
            };
            if format == OutputFormat::Ndjson {
                linkedin::render::stream_ndjson_items(&invites)?;
            } else {
                let summary = format!("Found {} invitations", invites.len());
                render_data(&invites, format, &summary)?;
            }
        }
        Commands::Invite(InviteArgs { command: InviteCommands::Triage { filter_has_note, interactive: _ } }) => {
            let conn_mgr = ChromiumConnectionManager::new(page, rule);
            let mut invites = conn_mgr.list_received_invitations(0, Some(50)).await?;
            if filter_has_note {
                invites.retain(|i| i.note.is_some());
            }
            if invites.is_empty() {
                println!("No pending invitations to triage.");
                return Ok(());
            }
            let mut reader = linkedin::infra::TerminalTriageReader::new();
            let triage_res = linkedin::domain::triage_invitations(&invites, &mut reader, &conn_mgr, false).await?;
            let summary = format!(
                "Triage completed!\nTotal Reviewed: {}\nAccepted: {}\nIgnored: {}\nSkipped: {}",
                triage_res.total_reviewed, triage_res.accepted, triage_res.ignored, triage_res.skipped
            );
            render_data(&triage_res, format, &summary)?;
        }
        Commands::Invite(InviteArgs { command: InviteCommands::Accept { handle } }) => {
            let conn_mgr = ChromiumConnectionManager::new(page, rule);
            conn_mgr.accept_invitation(&handle).await?;
            let res = serde_json::json!({ "status": "accepted", "invitation": handle });
            render_data(&res, format, &format!("Accepted invitation from {handle}"))?;
        }
        Commands::Invite(InviteArgs { command: InviteCommands::Withdraw { handle } }) => {
            let conn_mgr = ChromiumConnectionManager::new(page, rule);
            conn_mgr.withdraw_invitation(&handle).await?;
            let res = serde_json::json!({ "status": "withdrawn", "invitation": handle });
            render_data(&res, format, &format!("Withdrew invitation to {handle}"))?;
        }
        Commands::Invite(InviteArgs { command: InviteCommands::Ignore { handle } }) => {
            let conn_mgr = ChromiumConnectionManager::new(page, rule);
            conn_mgr.ignore_invitation(&handle).await?;
            let res = serde_json::json!({ "status": "ignored", "invitation": handle });
            render_data(&res, format, &format!("Ignored invitation from {handle}"))?;
        }
        Commands::Debug(DebugArgs { command: DebugCommands::Eval { script, js } })
        | Commands::BrowserEval { script, js } => {
            let code = js.or(script).unwrap_or_else(|| "window.location.href".to_string());
            let res = page
                .evaluate(code)
                .await
                .map_err(|e| linkedin::errors::AppError::Cdp(e.to_string()))?;
            let val: serde_json::Value = res.into_value().unwrap_or(serde_json::Value::Null);
            render_data(&val, format, &format!("{val}"))?;
        }
        Commands::Invites(args) => {
            let conn_mgr = ChromiumConnectionManager::new(page, rule);
            if let Some(target) = args.withdraw {
                conn_mgr.withdraw_invitation(&target).await?;
                let res = serde_json::json!({ "status": "withdrawn", "invitation": target });
                render_data(&res, format, &format!("Withdrew invitation to {target}"))?;
            } else if let Some(target) = args.accept {
                conn_mgr.accept_invitation(&target).await?;
                let res = serde_json::json!({ "status": "accepted", "invitation": target });
                render_data(&res, format, &format!("Accepted invitation from {target}"))?;
            } else if let Some(target) = args.ignore {
                conn_mgr.ignore_invitation(&target).await?;
                let res = serde_json::json!({ "status": "ignored", "invitation": target });
                render_data(&res, format, &format!("Ignored invitation from {target}"))?;
            } else if args.sent {
                let invites = conn_mgr.list_sent_invitations(args.scroll, args.limit).await?;
                let summary = format!("Found {} sent invitations", invites.len());
                render_data(&invites, format, &summary)?;
            } else {
                let invites = conn_mgr.list_received_invitations(args.scroll, args.limit).await?;
                let summary = format!("Found {} received invitations", invites.len());
                render_data(&invites, format, &summary)?;
            }
        }
        Commands::Completions { .. }
        | Commands::Manpage { .. }
        | Commands::Post(..)
        | Commands::Draft(..)
        | Commands::Tui(..)
        | Commands::Message(MessageArgs { command: MessageCommands::SendBatch { .. } })
        | Commands::Auth(AuthArgs {
            command: AuthCommands::Add { .. }
                | AuthCommands::List
                | AuthCommands::Switch { .. }
                | AuthCommands::Remove { .. },
        }) => unreachable!(),
        }
        Ok(())
    }
    .await;

    let close_res = session.close().await;

    command_result?;
    close_res?;

    Ok(())
}
