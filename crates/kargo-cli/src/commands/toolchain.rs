use miette::Result;

use crate::cli::ToolchainAction;

pub fn exec(action: ToolchainAction) -> Result<()> {
    match action {
        ToolchainAction::Install {
            version,
            jdk,
            android,
        } => kargo_ops::ops_toolchain::cmd_install(
            version.as_deref(),
            jdk.as_deref(),
            android.as_deref(),
        ),
        ToolchainAction::List => kargo_ops::ops_toolchain::cmd_list(),
        ToolchainAction::Remove {
            version,
            jdk,
            android,
        } => kargo_ops::ops_toolchain::cmd_remove(version.as_deref(), jdk.as_deref(), android),
        ToolchainAction::Use { version } => kargo_ops::ops_toolchain::cmd_use(&version),
        ToolchainAction::Path => {
            let home = kargo_ops::ops_toolchain::cmd_path()?;
            println!("{}", home.display());
            Ok(())
        }
    }
}
