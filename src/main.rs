use std::path::PathBuf;
use clap::{CommandFactory, Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use linkedin::browser::{BrowserConfig, BrowserSession};
use linkedin::domain::{
    ConnectionManager, ConnectionState, ContactInfoFetcher, DatePosted, ExperienceLevel, FeedSort,
    InboxFilter, InteractionRule, InvitationManager, JobDetails, JobSearchFilter, JobService,
    JobSummary, LinkedInDriver, MessagingService, PeopleSearchService, ProfileScraper, ReactionType,
    TimelinePost, TimelineService, WorkplaceType,
};
use linkedin::infra::{
    ChromiumConnectionManager, ChromiumContactInfoFetcher, ChromiumJobService,
    ChromiumLinkedInDriver, ChromiumMessagingService, ChromiumPeopleSearchService,
    ChromiumProfileScraper, ChromiumTimelineService,
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

    /// Timeline and home feed interactions (read updates, react, comment, repost)
    Feed(FeedArgs),

    /// Job search, details extraction, and saved jobs management
    Jobs(JobsArgs),

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
        /// Filter to profiles actively hiring
        #[arg(long)]
        actively_hiring: bool,
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
struct FeedArgs {
    #[command(subcommand)]
    command: FeedCommands,
}

#[derive(Subcommand, Debug)]
enum FeedCommands {
    /// List posts from the home feed
    List {
        /// Maximum number of posts to fetch (default: 10)
        #[arg(long, default_value = "10")]
        limit: usize,
        /// Feed sort order: recent or top (default: recent)
        #[arg(long, default_value = "recent")]
        sort: String,
    },
    /// List posts from a member's activity feed
    Member {
        /// Member public identifier or profile URL
        handle: String,
        /// Maximum number of posts to fetch (default: 10)
        #[arg(long, default_value = "10")]
        limit: usize,
    },
    /// Inspect a single post by URN
    Get {
        /// Post URN (e.g. urn:li:activity:123456789)
        urn: String,
    },
    /// List comments on a post
    Comments {
        /// Post URN
        urn: String,
        /// Maximum number of comments to fetch (default: 10)
        #[arg(long, default_value = "10")]
        limit: usize,
    },
    /// React to a post (like, celebrate, support, love, insightful, funny)
    React {
        /// Post URN
        urn: String,
        /// Reaction type: like, celebrate, support, love, insightful, funny
        #[arg(long, default_value = "like")]
        r#type: String,
    },
    /// Remove a reaction from a post
    Unreact {
        /// Post URN
        urn: String,
    },
    /// Add a comment to a post
    Comment {
        /// Post URN
        urn: String,
        /// Comment text
        #[arg(long)]
        text: String,
    },
    /// Repost an update (instant repost or quote post with commentary)
    Repost {
        /// Post URN
        urn: String,
        /// Optional commentary note for a quote repost
        #[arg(long)]
        note: Option<String>,
    },
}

#[derive(clap::Args, Debug)]
struct JobsArgs {
    #[command(subcommand)]
    command: JobsCommands,
}

#[derive(Subcommand, Debug)]
enum JobsCommands {
    /// Search job postings with faceted filters
    Search {
        /// Keywords or job title query
        keywords: Option<String>,
        /// Location filter (e.g. "Remote", "San Francisco, CA", "Portugal")
        #[arg(long)]
        location: Option<String>,
        /// Workplace flexibility policy: onsite, remote, hybrid (comma-separated)
        #[arg(long, value_delimiter = ',')]
        workplace: Vec<String>,
        /// Timeframe posted: past-24h, past-week, past-month, any
        #[arg(long)]
        posted: Option<String>,
        /// Experience level: internship, entry, associate, mid-senior, director, executive (comma-separated)
        #[arg(long, value_delimiter = ',')]
        experience: Vec<String>,
        /// Filter for LinkedIn Easy Apply postings
        #[arg(long)]
        easy_apply: bool,
        /// Maximum number of job postings to retrieve (default: 20)
        #[arg(long, default_value = "20")]
        limit: usize,
        /// Results page number (default: 1)
        #[arg(long, default_value = "1")]
        page: u32,
    },
    /// Inspect full details for a job posting
    Get {
        /// Job posting ID or URL (e.g. 4123456789 or https://www.linkedin.com/jobs/view/4123456789)
        id: String,
    },
    /// Save a job posting to your saved jobs list
    Save {
        /// Job posting ID or URL
        id: String,
    },
    /// Remove a job posting from your saved jobs list
    Unsave {
        /// Job posting ID or URL
        id: String,
    },
    /// List your saved job postings
    Saved {
        /// Maximum number of saved jobs to list (default: 20)
        #[arg(long, default_value = "20")]
        limit: usize,
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
        /// Optional target URL to navigate to before evaluating
        #[arg(long)]
        url: Option<String>,
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
            Commands::Feed(FeedArgs { command: FeedCommands::List { limit, sort } }) => {
                let sort_enum = sort.parse::<FeedSort>().unwrap_or_default();
                let mock_posts = vec![
                    TimelinePost {
                        urn: "urn:li:activity:7100000000000000001".to_string(),
                        author_name: "Tech Lead".to_string(),
                        author_handle: Some("tech-lead-sample".to_string()),
                        author_headline: Some("Building resilient systems".to_string()),
                        text: "Excited to share our latest milestone with the open-source community!".to_string(),
                        created_at: Some("2h".to_string()),
                        reaction_count: 42,
                        comment_count: 5,
                        repost_count: 2,
                        my_reaction: None,
                        media_urls: vec![],
                    },
                    TimelinePost {
                        urn: "urn:li:activity:7100000000000000002".to_string(),
                        author_name: "Staff Engineer".to_string(),
                        author_handle: Some("staff-engineer-sample".to_string()),
                        author_headline: Some("Distributed Systems & Rust".to_string()),
                        text: "Why deterministic testing matters for mission-critical software.".to_string(),
                        created_at: Some("5h".to_string()),
                        reaction_count: 128,
                        comment_count: 19,
                        repost_count: 12,
                        my_reaction: Some(ReactionType::Like),
                        media_urls: vec![],
                    },
                ];
                let mut posts = mock_posts;
                posts.truncate(*limit);
                if format == OutputFormat::Ndjson {
                    linkedin::render::stream_ndjson_items(&posts)?;
                } else {
                    let results = linkedin::domain::TimelineResult {
                        total: posts.len(),
                        posts,
                        sort: sort_enum,
                    };
                    render_data(&results, format, &format!("[DRY-RUN] Home Feed (sorted by {sort})"))?;
                }
                return Ok(());
            }
            Commands::Feed(FeedArgs { command: FeedCommands::Member { handle, limit } }) => {
                let mock_posts = vec![
                    TimelinePost {
                        urn: "urn:li:activity:7100000000000000003".to_string(),
                        author_name: handle.clone(),
                        author_handle: Some(handle.clone()),
                        author_headline: Some("Software Architect".to_string()),
                        text: format!("Recent post by {handle}"),
                        created_at: Some("1d".to_string()),
                        reaction_count: 15,
                        comment_count: 2,
                        repost_count: 0,
                        my_reaction: None,
                        media_urls: vec![],
                    },
                ];
                let mut posts = mock_posts;
                posts.truncate(*limit);
                if format == OutputFormat::Ndjson {
                    linkedin::render::stream_ndjson_items(&posts)?;
                } else {
                    let results = linkedin::domain::TimelineResult {
                        total: posts.len(),
                        posts,
                        sort: FeedSort::Recent,
                    };
                    render_data(&results, format, &format!("[DRY-RUN] Activity feed for {handle}"))?;
                }
                return Ok(());
            }
            Commands::Feed(FeedArgs { command: FeedCommands::Get { urn } }) => {
                let mock_post = TimelinePost {
                    urn: urn.clone(),
                    author_name: "Sample Author".to_string(),
                    author_handle: Some("sample-author".to_string()),
                    author_headline: Some("Engineering".to_string()),
                    text: "Sample post content for dry-run inspection.".to_string(),
                    created_at: Some("1d".to_string()),
                    reaction_count: 88,
                    comment_count: 12,
                    repost_count: 4,
                    my_reaction: None,
                    media_urls: vec![],
                };
                render_data(&mock_post, format, &format!("[DRY-RUN] Post details for {urn}"))?;
                return Ok(());
            }
            Commands::Feed(FeedArgs { command: FeedCommands::Comments { urn, limit } }) => {
                let mock_comments = vec![
                    linkedin::domain::PostComment {
                        urn: "urn:li:comment:1".to_string(),
                        author_name: "Alice Reviewer".to_string(),
                        author_handle: Some("alice-reviewer".to_string()),
                        text: "Great post, thanks for sharing!".to_string(),
                        created_at: Some("1h".to_string()),
                    },
                ];
                let mut comments = mock_comments;
                comments.truncate(*limit);
                if format == OutputFormat::Ndjson {
                    linkedin::render::stream_ndjson_items(&comments)?;
                } else {
                    render_data(&comments, format, &format!("[DRY-RUN] Comments for {urn}"))?;
                }
                return Ok(());
            }
            Commands::Feed(FeedArgs { command: FeedCommands::React { urn, r#type } }) => {
                let reaction = r#type.parse::<ReactionType>()?;
                DryRunAction {
                    resource: "feed".to_string(),
                    action: "react".to_string(),
                    target: Some(urn.clone()),
                    parameters: serde_json::json!({
                        "reaction": reaction.as_str(),
                        "post_urn": urn,
                        "action": "react_to_post"
                    }),
                    dry_run: true,
                }
            }
            Commands::Feed(FeedArgs { command: FeedCommands::Unreact { urn } }) => DryRunAction {
                resource: "feed".to_string(),
                action: "unreact".to_string(),
                target: Some(urn.clone()),
                parameters: serde_json::json!({
                    "post_urn": urn,
                    "action": "remove_reaction_from_post"
                }),
                dry_run: true,
            },
            Commands::Feed(FeedArgs { command: FeedCommands::Comment { urn, text } }) => {
                linkedin::domain::validate_comment_content(text)?;
                DryRunAction {
                    resource: "feed".to_string(),
                    action: "comment".to_string(),
                    target: Some(urn.clone()),
                    parameters: serde_json::json!({
                        "post_urn": urn,
                        "text": text,
                        "length": text.len(),
                        "action": "comment_on_post"
                    }),
                    dry_run: true,
                }
            }
            Commands::Feed(FeedArgs { command: FeedCommands::Repost { urn, note } }) => DryRunAction {
                resource: "feed".to_string(),
                action: "repost".to_string(),
                target: Some(urn.clone()),
                parameters: serde_json::json!({
                    "post_urn": urn,
                    "commentary": note,
                    "action": if note.is_some() { "quote_repost" } else { "instant_repost" }
                }),
                dry_run: true,
            },
            Commands::Jobs(JobsArgs { command: JobsCommands::Search { keywords, location, workplace, posted, experience, easy_apply, limit, page } }) => {
                let mut wp_enums = Vec::new();
                for w in workplace {
                    wp_enums.push(w.parse::<WorkplaceType>()?);
                }
                let posted_enum = if let Some(p) = posted {
                    Some(p.parse::<DatePosted>()?)
                } else {
                    None
                };
                let mut exp_enums = Vec::new();
                for e in experience {
                    exp_enums.push(e.parse::<ExperienceLevel>()?);
                }

                let filter = JobSearchFilter {
                    keywords: keywords.clone(),
                    location: location.clone(),
                    workplace: wp_enums,
                    date_posted: posted_enum,
                    experience: exp_enums,
                    easy_apply: *easy_apply,
                    limit: *limit,
                    page: *page,
                };

                let mock_jobs = vec![
                    JobSummary {
                        id: "4100000001".to_string(),
                        urn: "urn:li:jobPosting:4100000001".to_string(),
                        title: "Staff AI Systems Engineer".to_string(),
                        company_name: "Anthropic".to_string(),
                        company_handle: Some("anthropic-ai".to_string()),
                        location: "San Francisco, CA (Hybrid)".to_string(),
                        workplace_type: Some("Hybrid".to_string()),
                        posted_at: Some("2d".to_string()),
                        is_easy_apply: true,
                        job_url: "https://www.linkedin.com/jobs/view/4100000001/".to_string(),
                    },
                    JobSummary {
                        id: "4100000002".to_string(),
                        urn: "urn:li:jobPosting:4100000002".to_string(),
                        title: "Senior Rust & Distributed Systems Engineer".to_string(),
                        company_name: "Cloudflare".to_string(),
                        company_handle: Some("cloudflare".to_string()),
                        location: "Remote, US".to_string(),
                        workplace_type: Some("Remote".to_string()),
                        posted_at: Some("1w".to_string()),
                        is_easy_apply: false,
                        job_url: "https://www.linkedin.com/jobs/view/4100000002/".to_string(),
                    },
                ];

                let mut jobs = mock_jobs;
                jobs.truncate(*limit);

                if format == OutputFormat::Ndjson {
                    linkedin::render::stream_ndjson_items(&jobs)?;
                } else {
                    let result = linkedin::domain::JobSearchResult {
                        total: jobs.len(),
                        jobs,
                        page: *page,
                        query: keywords.clone(),
                    };
                    render_data(&result, format, &format!("[DRY-RUN] Job search URL: {}", filter.build_url()))?;
                }
                return Ok(());
            }
            Commands::Jobs(JobsArgs { command: JobsCommands::Get { id } }) => {
                let norm_id = ChromiumJobService::normalize_job_id(id);
                let mock_details = JobDetails {
                    id: norm_id.clone(),
                    urn: format!("urn:li:jobPosting:{norm_id}"),
                    title: "Senior AI Systems Engineer".to_string(),
                    company: linkedin::domain::CompanySummary {
                        name: "Anthropic".to_string(),
                        handle: Some("anthropic-ai".to_string()),
                        url: Some("https://www.linkedin.com/company/anthropic-ai".to_string()),
                    },
                    location: "San Francisco, CA (Hybrid)".to_string(),
                    workplace_type: Some("Hybrid".to_string()),
                    posted_at: Some("3 days ago".to_string()),
                    applicant_count: Some(64),
                    is_easy_apply: true,
                    salary_range: Some("$220,000 - $310,000 / year".to_string()),
                    employment_type: Some("Full-time".to_string()),
                    experience_level: Some("Mid-Senior level".to_string()),
                    required_skills: vec!["Rust".to_string(), "Python".to_string(), "LLM Inference".to_string(), "Kubernetes".to_string()],
                    hiring_team: vec![linkedin::domain::HiringMember {
                        name: "Sarah Talent".to_string(),
                        handle: Some("sarah-talent".to_string()),
                        headline: Some("Staff Technical Recruiter @ Anthropic".to_string()),
                        profile_url: Some("https://www.linkedin.com/in/sarah-talent".to_string()),
                    }],
                    description_markdown: "### About the Role\nWe are looking for an experienced systems engineer to scale AI agent infrastructure.".to_string(),
                    job_url: format!("https://www.linkedin.com/jobs/view/{norm_id}/"),
                };
                render_data(&mock_details, format, &format!("[DRY-RUN] Job details for {norm_id}"))?;
                return Ok(());
            }
            Commands::Jobs(JobsArgs { command: JobsCommands::Save { id } }) => {
                let norm_id = ChromiumJobService::normalize_job_id(id);
                DryRunAction {
                    resource: "jobs".to_string(),
                    action: "save".to_string(),
                    target: Some(norm_id.clone()),
                    parameters: serde_json::json!({
                        "job_id": norm_id,
                        "action": "save_job_posting"
                    }),
                    dry_run: true,
                }
            }
            Commands::Jobs(JobsArgs { command: JobsCommands::Unsave { id } }) => {
                let norm_id = ChromiumJobService::normalize_job_id(id);
                DryRunAction {
                    resource: "jobs".to_string(),
                    action: "unsave".to_string(),
                    target: Some(norm_id.clone()),
                    parameters: serde_json::json!({
                        "job_id": norm_id,
                        "action": "unsave_job_posting"
                    }),
                    dry_run: true,
                }
            }
            Commands::Jobs(JobsArgs { command: JobsCommands::Saved { limit } }) => {
                let mock_saved = vec![
                    JobSummary {
                        id: "4100000001".to_string(),
                        urn: "urn:li:jobPosting:4100000001".to_string(),
                        title: "Staff AI Systems Engineer".to_string(),
                        company_name: "Anthropic".to_string(),
                        company_handle: Some("anthropic-ai".to_string()),
                        location: "San Francisco, CA (Hybrid)".to_string(),
                        workplace_type: Some("Hybrid".to_string()),
                        posted_at: Some("Saved 2d ago".to_string()),
                        is_easy_apply: true,
                        job_url: "https://www.linkedin.com/jobs/view/4100000001/".to_string(),
                    },
                ];
                let mut saved = mock_saved;
                saved.truncate(*limit);
                if format == OutputFormat::Ndjson {
                    linkedin::render::stream_ndjson_items(&saved)?;
                } else {
                    render_data(&saved, format, "[DRY-RUN] Saved job postings")?;
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
    let target_url = match &cli.command {
        Commands::Message(..) | Commands::Inbox(..) | Commands::Thread { .. } => "https://www.linkedin.com/messaging/".to_string(),
        Commands::Search(..) => "https://www.linkedin.com/search/".to_string(),
        Commands::Connection(ConnectionArgs { command: ConnectionCommands::Request { handle, .. } })
        | Commands::Connect { handle, .. } => format!("https://www.linkedin.com/in/{handle}/"),
        Commands::Profile(ProfileArgs { command: ProfileCommands::Get { handle, .. } | ProfileCommands::Contact { handle, .. } | ProfileCommands::Status { handle } })
        | Commands::ContactInfo { handle, .. } => format!("https://www.linkedin.com/in/{handle}/"),
        Commands::Jobs(JobsArgs { command: JobsCommands::Get { id } })
        | Commands::Jobs(JobsArgs { command: JobsCommands::Save { id } })
        | Commands::Jobs(JobsArgs { command: JobsCommands::Unsave { id } }) => {
            let norm_id = ChromiumJobService::normalize_job_id(id);
            format!("https://www.linkedin.com/jobs/view/{norm_id}/")
        }
        Commands::Jobs(JobsArgs { command: JobsCommands::Search { .. } }) => "https://www.linkedin.com/jobs/search/".to_string(),
        Commands::Jobs(JobsArgs { command: JobsCommands::Saved { .. } }) => "https://www.linkedin.com/my-items/saved-jobs/".to_string(),
        Commands::Debug(DebugArgs { command: DebugCommands::Eval { url: Some(u), .. } }) => u.clone(),
        _ => "https://www.linkedin.com/feed/".to_string(),
    };
    let page = session.get_or_create_page(&target_url).await?;

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
            let conn_mgr = ChromiumConnectionManager::new(page.clone(), rule);
            let status_res = conn_mgr.send_connection_request(&handle, note.as_deref()).await;
            
            // Clean up profile page tab to avoid accumulating tabs across repeated connection dispatches
            let _ = session.close_page(&page).await;

            let status = status_res?;
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
        Commands::Search(SearchArgs { command: SearchCommands::People { keywords, network, location, hiring_for, actively_hiring, page: p } }) => {
            let search_svc = ChromiumPeopleSearchService::new(page, rule);
            let results = search_svc.search_people(&keywords, &network, &location, &hiring_for, actively_hiring, p).await?;
            if format == OutputFormat::Ndjson {
                linkedin::render::stream_ndjson_items(&results.profiles)?;
            } else {
                let summary = format!("Found {} profiles for query '{}' (page {})", results.profiles.len(), results.query, results.page);
                render_data(&results, format, &summary)?;
            }
        }
        Commands::Feed(FeedArgs { command }) => {
            let timeline_svc = ChromiumTimelineService::new(page, rule);
            match command {
                FeedCommands::List { limit, sort } => {
                    let sort_enum = sort.parse::<FeedSort>()?;
                    let posts = timeline_svc.list_feed(limit, sort_enum).await?;
                    if format == OutputFormat::Ndjson {
                        linkedin::render::stream_ndjson_items(&posts)?;
                    } else {
                        let summary = format!("Found {} feed post(s) (sorted by {sort})", posts.len());
                        let result = linkedin::domain::TimelineResult {
                            total: posts.len(),
                            posts,
                            sort: sort_enum,
                        };
                        render_data(&result, format, &summary)?;
                    }
                }
                FeedCommands::Member { handle, limit } => {
                    let posts = timeline_svc.list_member_feed(&handle, limit).await?;
                    if format == OutputFormat::Ndjson {
                        linkedin::render::stream_ndjson_items(&posts)?;
                    } else {
                        let summary = format!("Found {} activity post(s) for member {handle}", posts.len());
                        let result = linkedin::domain::TimelineResult {
                            total: posts.len(),
                            posts,
                            sort: FeedSort::Recent,
                        };
                        render_data(&result, format, &summary)?;
                    }
                }
                FeedCommands::Get { urn } => {
                    let post = timeline_svc.get_post(&urn).await?;
                    let summary = format!(
                        "Post: {}\nAuthor: {} ({})\nReactions: {} | Comments: {} | Reposts: {}\nText:\n{}",
                        post.urn,
                        post.author_name,
                        post.author_handle.as_deref().unwrap_or("N/A"),
                        post.reaction_count,
                        post.comment_count,
                        post.repost_count,
                        post.text
                    );
                    render_data(&post, format, &summary)?;
                }
                FeedCommands::Comments { urn, limit } => {
                    let comments = timeline_svc.list_comments(&urn, limit).await?;
                    if format == OutputFormat::Ndjson {
                        linkedin::render::stream_ndjson_items(&comments)?;
                    } else {
                        let summary = format!("Found {} comment(s) on post {urn}", comments.len());
                        render_data(&comments, format, &summary)?;
                    }
                }
                FeedCommands::React { urn, r#type } => {
                    let reaction = r#type.parse::<ReactionType>()?;
                    let res = timeline_svc.react(&urn, reaction).await?;
                    let summary = format!("Reacted with {} to post {}", reaction.display_label(), urn);
                    render_data(&res, format, &summary)?;
                }
                FeedCommands::Unreact { urn } => {
                    let res = timeline_svc.unreact(&urn).await?;
                    let summary = format!("Removed reaction from post {}", urn);
                    render_data(&res, format, &summary)?;
                }
                FeedCommands::Comment { urn, text } => {
                    let res = timeline_svc.add_comment(&urn, &text).await?;
                    let summary = format!("Commented on post {}: \"{}\"", urn, text);
                    render_data(&res, format, &summary)?;
                }
                FeedCommands::Repost { urn, note } => {
                    let res = timeline_svc.repost(&urn, note.as_deref()).await?;
                    let summary = if let Some(n) = note {
                        format!("Quote reposted {urn} with thoughts: \"{n}\"")
                    } else {
                        format!("Instantly reposted update {urn}")
                    };
                    render_data(&res, format, &summary)?;
                }
            }
        }
        Commands::Jobs(JobsArgs { command }) => {
            let jobs_svc = ChromiumJobService::new(page, rule);
            match command {
                JobsCommands::Search { keywords, location, workplace, posted, experience, easy_apply, limit, page: p } => {
                    let mut wp_enums = Vec::new();
                    for w in workplace {
                        wp_enums.push(w.parse::<WorkplaceType>()?);
                    }
                    let posted_enum = if let Some(posted_str) = posted {
                        Some(posted_str.parse::<DatePosted>()?)
                    } else {
                        None
                    };
                    let mut exp_enums = Vec::new();
                    for e in experience {
                        exp_enums.push(e.parse::<ExperienceLevel>()?);
                    }

                    let filter = JobSearchFilter {
                        keywords,
                        location,
                        workplace: wp_enums,
                        date_posted: posted_enum,
                        experience: exp_enums,
                        easy_apply,
                        limit,
                        page: p,
                    };

                    let result = jobs_svc.search_jobs(&filter).await?;
                    if format == OutputFormat::Ndjson {
                        linkedin::render::stream_ndjson_items(&result.jobs)?;
                    } else {
                        let summary = format!("Found {} job(s) for query '{}' (page {})", result.jobs.len(), result.query.as_deref().unwrap_or(""), result.page);
                        render_data(&result, format, &summary)?;
                    }
                }
                JobsCommands::Get { id } => {
                    let details = jobs_svc.get_job(&id).await?;
                    let summary = format!(
                        "Job: {} ({})\nCompany: {} | Location: {}\nWorkplace: {} | Posted: {}\nApplicants: {} | Easy Apply: {}\nSalary: {}\nSkills: {}\n\nDescription:\n{}",
                        details.title,
                        details.id,
                        details.company.name,
                        details.location,
                        details.workplace_type.as_deref().unwrap_or("N/A"),
                        details.posted_at.as_deref().unwrap_or("N/A"),
                        details.applicant_count.map(|c| c.to_string()).unwrap_or_else(|| "N/A".to_string()),
                        details.is_easy_apply,
                        details.salary_range.as_deref().unwrap_or("N/A"),
                        details.required_skills.join(", "),
                        details.description_markdown
                    );
                    render_data(&details, format, &summary)?;
                }
                JobsCommands::Save { id } => {
                    let res = jobs_svc.save_job(&id).await?;
                    let summary = format!("Job {}: {}", res.job_id, res.message);
                    render_data(&res, format, &summary)?;
                }
                JobsCommands::Unsave { id } => {
                    let res = jobs_svc.unsave_job(&id).await?;
                    let summary = format!("Job {}: {}", res.job_id, res.message);
                    render_data(&res, format, &summary)?;
                }
                JobsCommands::Saved { limit } => {
                    let jobs = jobs_svc.list_saved_jobs(limit).await?;
                    if format == OutputFormat::Ndjson {
                        linkedin::render::stream_ndjson_items(&jobs)?;
                    } else {
                        let summary = format!("Found {} saved job(s)", jobs.len());
                        render_data(&jobs, format, &summary)?;
                    }
                }
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
        Commands::Debug(DebugArgs { command: DebugCommands::Eval { script, js, .. } })
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
