use std::io::Write;

use console::Style;
use indicatif::{ProgressBar, ProgressStyle};

/// Print a Cargo-style status line: `    Compiling my-app v0.1.0`
///
/// The `label` is right-padded to 12 characters and printed in bold green,
/// followed by the `message` in the default terminal colour.
pub fn status(label: &str, message: &str) {
    let green_bold = Style::new().green().bold();
    let _ = writeln!(
        std::io::stderr(),
        "{:>12} {message}",
        green_bold.apply_to(label),
    );
}

/// Like [`status`] but uses bold cyan for informational (non-action) messages.
pub fn status_info(label: &str, message: &str) {
    let cyan_bold = Style::new().cyan().bold();
    let _ = writeln!(
        std::io::stderr(),
        "{:>12} {message}",
        cyan_bold.apply_to(label),
    );
}

/// Print a warning-style status line (bold yellow label).
pub fn status_warn(label: &str, message: &str) {
    let yellow_bold = Style::new().yellow().bold();
    let _ = writeln!(
        std::io::stderr(),
        "{:>12} {message}",
        yellow_bold.apply_to(label),
    );
}

/// Create an animated spinner with the given message for indeterminate progress.
///
/// The spinner ticks automatically and should be finished with
/// [`ProgressBar::finish_with_message`] or [`ProgressBar::finish_and_clear`].
pub fn spinner(message: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .expect("valid template"),
    );
    pb.set_message(message.to_string());
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    pb
}

/// Create a progress bar with the given length and message for determinate progress.
pub fn progress_bar(len: u64, message: &str) -> ProgressBar {
    let pb = ProgressBar::new(len);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{msg} [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
            .expect("valid template")
            .progress_chars("=> "),
    );
    pb.set_message(message.to_string());
    pb
}
