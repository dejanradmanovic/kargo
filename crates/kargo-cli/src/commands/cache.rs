//! Cache command implementation.

use miette::Result;

use crate::cli::CacheAction;

pub fn exec(action: CacheAction) -> Result<()> {
    match action {
        CacheAction::Stats => kargo_ops::ops_cache::stats(),
        CacheAction::Clean => kargo_ops::ops_cache::clean(),
        CacheAction::Push => {
            eprintln!("Remote cache push is not yet implemented.");
            Ok(())
        }
        CacheAction::StopDaemon => kargo_ops::ops_cache::stop_daemon(),
    }
}
