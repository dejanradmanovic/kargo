//! Kargo CLI binary.
//!
//! This is the entry point for the `kargo` command-line tool. It initializes
//! logging via `tracing`, parses arguments with `clap`, and dispatches to
//! the appropriate command handler.

mod cli;
mod commands;

use miette::Result;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let args = cli::parse();
    commands::dispatch(args).await
}
