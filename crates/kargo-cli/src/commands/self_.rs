use miette::Result;

use crate::cli::SelfAction;

pub fn exec(action: SelfAction) -> Result<()> {
    match action {
        SelfAction::Info => kargo_ops::ops_self::cmd_info(env!("CARGO_PKG_VERSION")),
        SelfAction::Clean => kargo_ops::ops_self::cmd_clean(),
        SelfAction::Update { check } => {
            kargo_ops::ops_self::cmd_update(env!("CARGO_PKG_VERSION"), check)
        }
    }
}
