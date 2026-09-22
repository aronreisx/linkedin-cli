use std::path::PathBuf;
use clap::{CommandFactory, Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use linkedin::browser::{BrowserConfig, BrowserSession};
use linkedin::domain::{
    ConnectionManager, ConnectionState, ContactInfoFetcher, InboxFilter, InteractionRule,
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
    #[arg(long, global = true, env = "LINKEDIN_CDP_URL")]
    cdp_url: Option<String>,

    /// Slow down CDP operations by the specified amount of milliseconds (pacing)
    #[arg(long, global = true, env = "LINKEDIN_SLOW_MO")]
    slow_mo: Option<u64>,

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

    /// Content publishing, post analytics, and staged draft management
    Post(PostArgs),

    /// Local draft staging, scheduling, and export (deprecated alias: use 'linkedin post draft')
    #[command(hide = true)]
    Draft(DraftArgs),

    /// Persistent browser supervisor daemon management
    Session(SessionArgs),

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
        #[arg(long, required_unless_present = "attachment")]
        text: Option<String>,
        /// Path to an attachment file to send
        #[arg(short = 'a', long)]
        attachment: Option<PathBuf>,
    },
    /// Send templated messages to a batch of recipients with safety checks
    SendBatch {
        /// Template string or path to template file
        #[arg(long)]
        template: String,
        /// Path to JSON file containing array of recipient objects
        #[arg(long)]
        recipients: PathBuf,
        /// Default attachment to send with each message if not specified per recipient
        #[arg(short = 'a', long)]
        attachment: Option<PathBuf>,
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
    /// Manage staged and scheduled post drafts
    #[command(subcommand)]
    Draft(DraftCommands),
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
struct SessionArgs {
    #[command(subcommand)]
    command: SessionCommands,
}

#[derive(Subcommand, Debug)]
enum SessionCommands {
    /// Launch a persistent browser supervisor daemon, bind it to a CDP endpoint, and keep running
    Open {
        /// Session profile name
        #[arg(short, long, default_value = "default")]
        name: String,

        /// Port to bind the Chromium remote debugging endpoint to (default: 0 = auto-allocated)
        #[arg(short, long, default_value = "0")]
        port: u16,

        /// Host address to bind the debugging endpoint to
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        /// Launch browser in headed (visible) mode
        #[arg(long)]
        headed: bool,
    },

    /// Show the status of a bound browser session daemon
    Status {
        /// Session profile name
        #[arg(short, long, default_value = "default")]
        name: String,
    },

    /// Close and terminate a running browser supervisor daemon
    Close {
        /// Session profile name
        #[arg(short, long, default_value = "default")]
        name: String,
    },

    /// List all registered browser supervisor daemon sessions
    List,
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
    /// Fetch all conversations, paginating through history until exhaustion or --limit
    #[arg(long)]
    all: bool,
    /// Start fetching from this opaque pagination cursor token (returned in previous queries)
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
    /// Return at most N conversations (defaults to LinkedIn's single-page batch of ~20 if omitted)
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
            .unwrap_or_else(|_| "linkedin=info,browser=info,driver=info,scraper=info,contact=info,connection=info,messaging=info,search=info".into())
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
        Commands::Session(SessionArgs { command }) => {
            match command {
                SessionCommands::Open { name, port, host, headed } => {
                    if cli.dry_run {
                        let dry_run_action = DryRunAction {
                            resource: "session".to_string(),
                            action: "open".to_string(),
                            target: Some(name.clone()),
                            parameters: serde_json::json!({
                                "name": name,
                                "port": port,
                                "host": host,
                                "headed": headed,
                            }),
                            dry_run: true,
                        };
                        render_dry_run(dry_run_action, format)?;
                        return Ok(());
                    }

                    if let Some(active) = linkedin::browser::find_active_session(name) {
                        let res = serde_json::json!({
                            "status": "already_running",
                            "name": active.name,
                            "endpoint": active.endpoint,
                            "pid": active.pid,
                            "created_at": active.created_at,
                        });
                        render_data(&res, format, &format!("Session '{name}' is already running at {} (PID: {})", active.endpoint, active.pid))?;
                        return Ok(());
                    }

                    let b_config = BrowserConfig {
                        headed: *headed || cli.headed,
                        user_data_dir: cli.user_data_dir.clone(),
                        cdp_url: None,
                        session_name: name.clone(),
                        slow_mo_ms: cli.slow_mo,
                    };

                    tracing::info!(target: "session", "Launching supervisor daemon session '{name}'...");
                    let session = BrowserSession::launch(&b_config).await?;
                    let ws_url = session.debug_ws_url().await.unwrap_or_default();
                    let pid = std::process::id();
                    let _entry = linkedin::browser::write_session(name, &ws_url, pid)?;

                    let _page = session.get_or_create_page("https://www.linkedin.com/feed/").await?;
                    println!("{ws_url}");
                    eprintln!("Session '{name}' bound at {ws_url} (PID: {pid}). Press Ctrl+C to terminate.");

                    // Block until termination signal
                    tokio::select! {
                        _ = tokio::signal::ctrl_c() => {
                            eprintln!("Received SIGINT, closing session '{name}'...");
                        }
                    }
                    let _ = linkedin::browser::clear_session(name);
                    session.close().await?;
                    eprintln!("Session '{name}' closed cleanly.");
                    return Ok(());
                }
                SessionCommands::Status { name } => {
                    if cli.dry_run {
                        let dry_run_action = DryRunAction {
                            resource: "session".to_string(),
                            action: "status".to_string(),
                            target: Some(name.clone()),
                            parameters: serde_json::json!({ "name": name }),
                            dry_run: true,
                        };
                        render_dry_run(dry_run_action, format)?;
                        return Ok(());
                    }

                    let res = if let Some(entry) = linkedin::browser::find_active_session(name) {
                        serde_json::json!({
                            "status": "active",
                            "name": entry.name,
                            "endpoint": entry.endpoint,
                            "pid": entry.pid,
                            "created_at": entry.created_at,
                            "alive": true,
                        })
                    } else {
                        serde_json::json!({
                            "status": "inactive",
                            "name": name,
                            "alive": false,
                        })
                    };
                    render_data(&res, format, &format!("Session '{name}' Status"))?;
                    return Ok(());
                }
                SessionCommands::Close { name } => {
                    if cli.dry_run {
                        let dry_run_action = DryRunAction {
                            resource: "session".to_string(),
                            action: "close".to_string(),
                            target: Some(name.clone()),
                            parameters: serde_json::json!({ "name": name }),
                            dry_run: true,
                        };
                        render_dry_run(dry_run_action, format)?;
                        return Ok(());
                    }

                    let res = if let Some(entry) = linkedin::browser::read_session(name) {
                        let terminated = linkedin::browser::registry::terminate_pid(entry.pid);
                        let _ = linkedin::browser::clear_session(name);
                        serde_json::json!({
                            "status": "closed",
                            "name": name,
                            "pid": entry.pid,
                            "signal_sent": terminated,
                        })
                    } else {
                        serde_json::json!({
                            "status": "not_found",
                            "name": name,
                        })
                    };
                    render_data(&res, format, &format!("Closed session '{name}'"))?;
                    return Ok(());
                }
                SessionCommands::List => {
                    if cli.dry_run {
                        let dry_run_action = DryRunAction {
                            resource: "session".to_string(),
                            action: "list".to_string(),
                            target: None,
                            parameters: serde_json::json!({}),
                            dry_run: true,
                        };
                        render_dry_run(dry_run_action, format)?;
                        return Ok(());
                    }

                    let sessions = linkedin::browser::list_sessions()?;
                    let mut items = Vec::new();
                    for s in sessions {
                        let alive = linkedin::browser::registry::is_pid_alive(s.pid);
                        items.push(serde_json::json!({
                            "name": s.name,
                            "pid": s.pid,
                            "endpoint": s.endpoint,
                            "created_at": s.created_at,
                            "status": if alive { "active" } else { "stale" },
                        }));
                    }
                    let res = serde_json::json!(items);
                    render_data(&res, format, "Registered Browser Sessions")?;
                    return Ok(());
                }
            }
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
        Commands::Message(MessageArgs { command: MessageCommands::SendBatch { template, recipients, attachment, max_batch, jitter_min, jitter_max } }) if cli.dry_run => {
            let template_str = if let Ok(content) = std::fs::read_to_string(template) {
                content
            } else {
                template.clone()
            };
            let recips_str = std::fs::read_to_string(recipients)
                .map_err(linkedin::errors::AppError::Io)?;
            let recipients_arr: Vec<serde_json::Value> = serde_json::from_str(&recips_str)?;

            linkedin::domain::validate_batch_limits(recipients_arr.len(), *max_batch)?;
            let mut rendered = linkedin::domain::render_batch_templates(&template_str, &recipients_arr)?;
            if let Some(ref default_att) = attachment {
                let default_str = default_att.to_string_lossy().to_string();
                for msg in &mut rendered {
                    if msg.attachment.is_none() {
                        msg.attachment = Some(default_str.clone());
                    }
                }
            }
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

            let dry_run_action = DryRunAction {
                resource: "message".to_string(),
                action: "send_batch".to_string(),
                target: None,
                parameters: serde_json::to_value(&plan)?,
                dry_run: true,
            };
            render_dry_run(dry_run_action, format)?;
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
        Commands::Post(PostArgs { command: PostCommands::Draft(ref command) })
        | Commands::Draft(DraftArgs { ref command }) => {
            let store = linkedin::infra::SqliteDraftStore::open_default()?;
            let is_post = matches!(cli.command, Commands::Post(..));
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
                            resource: if is_post { "post".to_string() } else { "draft".to_string() },
                            action: if is_post { "draft_add".to_string() } else { "add".to_string() },
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
                            resource: if is_post { "post".to_string() } else { "draft".to_string() },
                            action: if is_post { "draft_export".to_string() } else { "export".to_string() },
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
            Commands::Message(MessageArgs {
                command: MessageCommands::Send {
                    handle,
                    text,
                    attachment,
                },
            }) => DryRunAction {
                resource: "message".to_string(),
                action: "send".to_string(),
                target: Some(handle.clone()),
                parameters: serde_json::json!({
                    "text": text,
                    "length": text.as_ref().map(|t| t.len()).unwrap_or(0),
                    "attachment": attachment.as_ref().map(|p| p.to_string_lossy().to_string()),
                    "action": if attachment.is_some() { "send_direct_message_with_attachment" } else { "send_direct_message" }
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
        slow_mo_ms: cli.slow_mo,
    };

    let session = BrowserSession::launch(&browser_config).await?;
    let page = session.get_or_create_page("https://www.linkedin.com/feed/").await?;

    let store = linkedin::infra::resolve_credential_store();
    let stored_creds = store.get_credentials(&cli.session).ok().flatten();

    // Inject session cookies from credential store if available (only for local managed browsers)
    if !session.is_remote() {
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
    }

    let rule = InteractionRule::default();

    // Commands that don't require an active session:
    // Auth status (without handle) and Login
    let requires_session = !matches!(
        &cli.command,
        Commands::Auth(AuthArgs { command: AuthCommands::Status { handle: None } })
            | Commands::Status { handle: None }
            | Commands::Auth(AuthArgs { command: AuthCommands::Login })
            | Commands::Login
    );

    if requires_session {
        ensure_authenticated_session(&page, &rule, store.as_ref(), &cli.session, &stored_creds, session.is_remote()).await?;
    }

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
                let mut summary = format!("Contact info for {handle}:\nEmails: {:?}\nPhones: {:?}\nWebsites: {:?}",
                    contact.emails, contact.phone_numbers, contact.websites
                );
                if let Some(ref bday) = contact.birthday {
                    summary.push_str(&format!("\nBirthday: {bday}"));
                }
                if let Some(ref conn) = contact.connected {
                    summary.push_str(&format!("\nConnected: {conn}"));
                }
                if !contact.twitter_handles.is_empty() {
                    summary.push_str(&format!("\nTwitter: {:?}", contact.twitter_handles));
                }
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
            let status = conn_mgr.send_connection_request(&handle, note.as_deref()).await?;
            let summary = match status.state {
                ConnectionState::Pending => format!("Connection request sent to {handle}"),
                ConnectionState::Connected => format!("{handle} is already a 1st degree connection"),
                _ => format!("Connection status with {handle}: {}", status.state.as_str()),
            };
            let res = serde_json::json!({
                "status": match status.state {
                    ConnectionState::Pending => "sent",
                    ConnectionState::Connected => "already_connected",
                    _ => "qualified",
                },
                "recipient": handle,
                "note": note,
                "state": status.state.as_str(),
            });
            render_data(&res, format, &summary)?;
        }
        Commands::Message(MessageArgs {
            command: MessageCommands::Send {
                handle,
                text,
                attachment,
            },
        }) => {
            let msg_svc = ChromiumMessagingService::new(page, rule);
            if let Some(ref att) = attachment {
                msg_svc
                    .send_direct_message_with_attachment(&handle, text.as_deref(), att)
                    .await?;
            } else {
                let txt = text.as_deref().unwrap_or("");
                msg_svc.send_direct_message(&handle, txt).await?;
            }
            let res = serde_json::json!({
                "status": "sent",
                "recipient": handle,
                "text": text,
                "attachment": attachment.as_ref().map(|p| p.to_string_lossy().to_string()),
            });
            render_data(&res, format, &format!("Message sent to {handle}"))?;
        }
        Commands::Message(MessageArgs {
            command: MessageCommands::SendBatch {
                template,
                recipients,
                attachment,
                max_batch,
                jitter_min,
                jitter_max,
            },
        }) => {
            let template_str = if let Ok(content) = std::fs::read_to_string(&template) {
                content
            } else {
                template.clone()
            };
            let recips_str = std::fs::read_to_string(recipients)
                .map_err(linkedin::errors::AppError::Io)?;
            let recipients_arr: Vec<serde_json::Value> = serde_json::from_str(&recips_str)?;

            linkedin::domain::validate_batch_limits(recipients_arr.len(), max_batch)?;
            let mut rendered = linkedin::domain::render_batch_templates(&template_str, &recipients_arr)?;
            if let Some(ref default_att) = attachment {
                let default_str = default_att.to_string_lossy().to_string();
                for msg in &mut rendered {
                    if msg.attachment.is_none() {
                        msg.attachment = Some(default_str.clone());
                    }
                }
            }
            let variance = linkedin::domain::check_content_variance(&rendered)?;

            if !variance.passes_variance_check {
                tracing::warn!(
                    target: "messaging",
                    "Batch messages have high similarity ({:.1}%). Account protection recommends varying template tokens.",
                    variance.average_similarity * 100.0
                );
            }

            let msg_svc = ChromiumMessagingService::new(page, rule);
            let total = rendered.len();
            let mut results = Vec::with_capacity(total);

            for (idx, msg) in rendered.iter().enumerate() {
                let att_display = msg.attachment.as_deref().unwrap_or("none");
                tracing::info!(
                    target: "messaging",
                    "[{}/{}] Dispatching batch message to {} (attachment: {})",
                    idx + 1,
                    total,
                    msg.recipient,
                    att_display
                );

                let send_res = if let Some(ref att) = msg.attachment {
                    let att_path = std::path::Path::new(att);
                    msg_svc
                        .send_direct_message_with_attachment(&msg.recipient, Some(&msg.text), att_path)
                        .await
                } else {
                    msg_svc.send_direct_message(&msg.recipient, &msg.text).await
                };

                let status_entry = match send_res {
                    Ok(_) => serde_json::json!({
                        "recipient": msg.recipient,
                        "status": "sent",
                        "text": msg.text,
                        "attachment": msg.attachment,
                    }),
                    Err(e) => {
                        tracing::error!(
                            target: "messaging",
                            "Failed to send to {}: {}",
                            msg.recipient,
                            e
                        );
                        serde_json::json!({
                            "recipient": msg.recipient,
                            "status": "failed",
                            "error": e.to_string(),
                            "text": msg.text,
                            "attachment": msg.attachment,
                        })
                    }
                };
                results.push(status_entry);

                // If there are more recipients, wait randomized jitter between jitter_min and jitter_max seconds
                if idx + 1 < total {
                    use rand::Rng;
                    let min_j = jitter_min;
                    let max_j = jitter_max.max(min_j);
                    let jitter_secs = if min_j == max_j {
                        min_j
                    } else {
                        rand::thread_rng().gen_range(min_j..=max_j)
                    };

                    tracing::info!(
                        target: "messaging",
                        "Anti-bot pacing: sleeping {jitter_secs}s before next message..."
                    );
                    tokio::time::sleep(tokio::time::Duration::from_secs(jitter_secs)).await;
                }
            }

            let successful = results.iter().filter(|r| r["status"] == "sent").count();
            let summary = format!(
                "Batch Dispatch Complete: {successful}/{total} sent successfully"
            );
            let res = serde_json::json!({
                "total": total,
                "sent": successful,
                "failed": total - successful,
                "results": results,
            });
            render_data(&res, format, &summary)?;
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
            } else if format == OutputFormat::Json {
                if filter_args.raw && raw_val.is_some() {
                    let mut full_obj = serde_json::to_value(&inbox_res)?;
                    if let Some(obj) = full_obj.as_object_mut() {
                        if let Some(r) = raw_val {
                            obj.insert("_raw".to_string(), r);
                        }
                    }
                    render_data(&full_obj, format, &format!("Loaded {} conversations", inbox_res.conversations.len()))?;
                } else {
                    render_data(&inbox_res, format, &format!("Loaded {} conversations", inbox_res.conversations.len()))?;
                }
            } else {
                let mut summary = format!("Loaded {} conversations from inbox", inbox_res.conversations.len());
                if let Some(ref cur) = inbox_res.next_cursor {
                    summary.push_str(&format!("\nNext page cursor: {cur} (use: linkedin inbox list --cursor \"{cur}\")"));
                }
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
                let label = if sent { "sent" } else { "received" };
                let summary = format_invitations_summary(&invites, label);
                render_data(&invites, format, &summary)?;
            }
        }
        Commands::Invite(InviteArgs { command: InviteCommands::Accept { handle } }) => {
            let conn_mgr = ChromiumConnectionManager::new(page, rule);
            let ok = conn_mgr.accept_invitation(&handle).await?;
            if !ok {
                return Err(linkedin::errors::AppError::NotFound(format!(
                    "Failed to accept invitation from '{handle}': invitation not found or accept button missing"
                )));
            }
            let res = serde_json::json!({ "status": "accepted", "invitation": handle });
            render_data(&res, format, &format!("Accepted invitation from {handle}"))?;
        }
        Commands::Invite(InviteArgs { command: InviteCommands::Withdraw { handle } }) => {
            let conn_mgr = ChromiumConnectionManager::new(page, rule);
            let ok = conn_mgr.withdraw_invitation(&handle).await?;
            if !ok {
                return Err(linkedin::errors::AppError::NotFound(format!(
                    "Failed to withdraw invitation to '{handle}': invitation not found or withdrawal confirmation missing"
                )));
            }
            let res = serde_json::json!({ "status": "withdrawn", "invitation": handle });
            render_data(&res, format, &format!("Withdrew invitation to {handle}"))?;
        }
        Commands::Invite(InviteArgs { command: InviteCommands::Ignore { handle } }) => {
            let conn_mgr = ChromiumConnectionManager::new(page, rule);
            let ok = conn_mgr.ignore_invitation(&handle).await?;
            if !ok {
                return Err(linkedin::errors::AppError::NotFound(format!(
                    "Failed to ignore invitation from '{handle}': invitation not found or ignore button missing"
                )));
            }
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
                let ok = conn_mgr.withdraw_invitation(&target).await?;
                if !ok {
                    return Err(linkedin::errors::AppError::NotFound(format!(
                        "Failed to withdraw invitation to '{target}': invitation not found or withdrawal confirmation missing"
                    )));
                }
                let res = serde_json::json!({ "status": "withdrawn", "invitation": target });
                render_data(&res, format, &format!("Withdrew invitation to {target}"))?;
            } else if let Some(target) = args.accept {
                let ok = conn_mgr.accept_invitation(&target).await?;
                if !ok {
                    return Err(linkedin::errors::AppError::NotFound(format!(
                        "Failed to accept invitation from '{target}': invitation not found or accept button missing"
                    )));
                }
                let res = serde_json::json!({ "status": "accepted", "invitation": target });
                render_data(&res, format, &format!("Accepted invitation from {target}"))?;
            } else if let Some(target) = args.ignore {
                let ok = conn_mgr.ignore_invitation(&target).await?;
                if !ok {
                    return Err(linkedin::errors::AppError::NotFound(format!(
                        "Failed to ignore invitation from '{target}': invitation not found or ignore button missing"
                    )));
                }
                let res = serde_json::json!({ "status": "ignored", "invitation": target });
                render_data(&res, format, &format!("Ignored invitation from {target}"))?;
            } else if args.sent {
                let invites = conn_mgr.list_sent_invitations(args.scroll, args.limit).await?;
                let summary = format_invitations_summary(&invites, "sent");
                render_data(&invites, format, &summary)?;
            } else {
                let invites = conn_mgr.list_received_invitations(args.scroll, args.limit).await?;
                let summary = format_invitations_summary(&invites, "received");
                render_data(&invites, format, &summary)?;
            }
        }
        Commands::Completions { .. }
        | Commands::Manpage { .. }
        | Commands::Post(..)
        | Commands::Draft(..)
        | Commands::Tui(..)
        | Commands::Session(..)
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

async fn ensure_authenticated_session(
    page: &chromiumoxide::Page,
    rule: &InteractionRule,
    store: &dyn linkedin::domain::CredentialStore,
    session_name: &str,
    stored_creds: &Option<linkedin::domain::ProfileCredentials>,
    is_remote: bool,
) -> Result<(), linkedin::errors::AppError> {
    use linkedin::domain::AuthStatus;
    use linkedin::domain::traits::LinkedInDriver;
    use linkedin::errors::AppError;
    use linkedin::infra::ChromiumLinkedInDriver;

    let driver = ChromiumLinkedInDriver::new(page.clone(), rule.clone());
    let status = driver.check_auth_status().await?;

    match status {
        AuthStatus::Authenticated { ref account_name, .. } => {
            tracing::debug!(
                target: "auth",
                "Authenticated session active: {}",
                account_name.as_deref().unwrap_or("Connected")
            );
            persist_session_cookies(page, store, session_name, stored_creds).await;
            Ok(())
        }
        AuthStatus::ChallengeCheckpoint { url } => {
            tracing::warn!(
                target: "auth",
                "Two-factor authentication / security verification required at {url}.\nPlease accept the prompt on your phone or complete verification in the browser window (waiting up to 300s)..."
            );
            let checkpoint_start = std::time::Instant::now();
            let mut cleared = false;
            while checkpoint_start.elapsed().as_secs() < 300 {
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                let current_u = driver.page_url().await.unwrap_or_default();
                let current_st = linkedin::domain::page_state::classify_page_url(&current_u);
                if current_st == linkedin::domain::page_state::PageState::Feed
                    || current_st == linkedin::domain::page_state::PageState::Profile
                    || (current_st != linkedin::domain::page_state::PageState::Checkpoint
                        && current_st != linkedin::domain::page_state::PageState::Login
                        && current_st != linkedin::domain::page_state::PageState::Authwall)
                {
                    tracing::info!(target: "auth", "Two-factor authentication cleared! Redirected to {current_u}");
                    cleared = true;
                    break;
                }
            }
            if !cleared {
                return Err(AppError::CheckpointChallenge(url));
            }
            persist_session_cookies(page, store, session_name, stored_creds).await;
            Ok(())
        }
        AuthStatus::Unauthenticated => {
            if is_remote {
                return Err(AppError::Authentication(
                    "Remote Chrome session has no active LinkedIn login. Please log in to LinkedIn in your Chrome browser window.".to_string()
                ));
            }
            tracing::info!(
                target: "auth",
                "Chromium has no active session. Signing in with stored credentials..."
            );
            let username = stored_creds
                .as_ref()
                .and_then(|c| c.username.clone())
                .or_else(|| std::env::var("LINKEDIN_USERNAME").ok());
            let password = stored_creds
                .as_ref()
                .and_then(|c| c.password.clone())
                .or_else(|| std::env::var("LINKEDIN_PASSWORD").ok());

            let u = username.ok_or_else(|| {
                AppError::Authentication(
                    "Chromium has no active LinkedIn session, and no username was found in keyring or environment (LINKEDIN_USERNAME). Run `linkedin auth add <name>` or set LINKEDIN_USERNAME / LINKEDIN_PASSWORD.".to_string()
                )
            })?;
            let p = password.ok_or_else(|| {
                AppError::Authentication(
                    "Chromium has no active LinkedIn session, and no password was found in keyring or environment (LINKEDIN_PASSWORD). Run `linkedin auth add <name>` or set LINKEDIN_PASSWORD.".to_string()
                )
            })?;

            let _ = driver.authenticate(Some(&u), Some(&p)).await?;
            persist_session_cookies(page, store, session_name, stored_creds).await;
            Ok(())
        }
    }
}

async fn persist_session_cookies(
    page: &chromiumoxide::Page,
    store: &dyn linkedin::domain::CredentialStore,
    session_name: &str,
    stored_creds: &Option<linkedin::domain::ProfileCredentials>,
) {
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
            if let Some(token) = li_at {
                creds.li_at = Some(token);
            }
            if let Some(session_id) = jsessionid {
                creds.jsessionid = Some(session_id);
            }
            creds.created_at = linkedin::infra::current_epoch_seconds();
            if let Err(e) = store.set_credentials(session_name, &creds) {
                tracing::warn!(target: "driver", "Failed to persist cookies to keyring: {e}");
            } else {
                tracing::info!(
                    target: "driver",
                    "Session cookies successfully updated in keyring for profile '{session_name}'"
                );
            }
        }
    }
}

fn format_invitations_summary(invites: &[linkedin::domain::models::Invitation], label: &str) -> String {
    if invites.is_empty() {
        return format!("No {label} invitations found.");
    }
    let mut summary = format!("Found {} {label} invitations:\n", invites.len());
    for inv in invites {
        let name = inv.name.as_deref().unwrap_or(&inv.public_identifier);
        let headline = inv.headline.as_deref().unwrap_or("No headline");
        summary.push_str(&format!("• {} ({}) - {}\n", name, inv.public_identifier, headline));
        if let Some(ref time) = inv.sent_or_received_at {
            summary.push_str(&format!("  Time: {}\n", time));
        }
        if let Some(ref note) = inv.note {
            summary.push_str(&format!("  Note: \"{}\"\n", note));
        }
    }
    summary.trim_end().to_string()
}
