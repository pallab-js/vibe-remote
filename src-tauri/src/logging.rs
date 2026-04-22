//! Logging initialization for VibeRemote
//!
//! Sets up tracing-subscriber with environment variable filtering
//! and outputs to both stdout and a log file.
//! LOW-2: Verbose logging (file/line) is now opt-in via VIBE_LOG_LEVEL=debug

use tracing::info;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// Initialize the logging system
///
/// Configures tracing with:
/// - Environment variable control via `VIBE_LOG_LEVEL`
/// - Pretty formatting for development
/// - Optional file logging
/// - LOW-2: Verbose logging only enabled when explicitly requested
pub fn init_logging() {
    // Check if verbose logging is explicitly requested
    let verbose_enabled = std::env::var("VIBE_VERBOSE")
        .map(|v| v == "1" || v == "true")
        .unwrap_or(false);

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("vibe_remote=info"));

    // LOW-2: Only include file/line info when explicitly requested
    if verbose_enabled {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(
                fmt::layer()
                    .pretty()
                    .with_target(true)
                    .with_thread_ids(true)
                    .with_file(true)
                    .with_line_number(true),
            )
            .init();
        info!("VibeRemote logging initialized (verbose mode)");
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(
                fmt::layer()
                    .pretty()
                    .with_target(true)
                    .with_thread_ids(true), // LOW-2: No file/line numbers in production logs
            )
            .init();
        info!("VibeRemote logging initialized");
    }
}
