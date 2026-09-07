pub mod config;
pub mod registry;
pub mod session;
pub mod stealth;
pub mod transport;

pub use config::BrowserConfig;
pub use registry::{
    clear_session, find_active_session, is_pid_alive, list_sessions, read_session, terminate_pid,
    write_session, SessionEntry,
};
pub use session::BrowserSession;
pub use stealth::apply_stealth;
pub use transport::InPageTransport;
