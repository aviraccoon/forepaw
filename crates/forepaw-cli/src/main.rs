//! A raccoon's paws on your desktop. Cross-platform automation CLI.
//!
//! Control any desktop application through accessibility trees, OCR, and
//! input simulation. Platform backends for macOS, Windows, and Linux are
//! selected at compile time via `#[cfg]`.

#![expect(
    clippy::print_stdout,
    reason = "CLI binary, stdout is the output channel"
)]

use clap::Parser;

mod cli;

/// Reset SIGPIPE to OS default so broken pipes silently terminate the process,
/// matching standard Unix CLI behavior. Rust's runtime ignores SIGPIPE, which
/// causes `println!`/`writeln!` to panic with `BrokenPipe` errors instead.
fn reset_sigpipe() {
    #[cfg(unix)]
    // SAFETY: libc::signal with SIG_DFL is a well-defined signal handler reset.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

use crate::cli::action::{
    Batch, Click, Drag, Hover, KeyboardType, OcrClick, Press, Scroll, Type, Wait,
};
use crate::cli::observation::{
    HitTest, ListApps, ListDisplays, ListWindows, Ocr, Screenshot, Snapshot,
};
use crate::cli::system::Permissions;
use crate::cli::GlobalArgs;
use forepaw::core::output_formatter::OutputFormat;

#[derive(Parser)]
#[command(
    name = "forepaw",
    about = "A raccoon's paws on your desktop. Cross-platform automation CLI.",
    version = concat!(env!("CARGO_PKG_VERSION"), " (", env!("FOREPAW_GIT_SHA"), ")"),
)]
struct App {
    #[arg(
        short,
        long,
        value_name = "FORMAT",
        default_value = "text",
        global = true,
        help = "Output format: text, json"
    )]
    format: OutputFormat,

    #[arg(
        short,
        long,
        global = true,
        help = "Show additional detail (native roles, attributes, identifiers)"
    )]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    Snapshot(Snapshot),
    Screenshot(Screenshot),
    #[command(name = "list-apps")]
    ListApps(ListApps),
    #[command(name = "list-windows")]
    ListWindows(ListWindows),
    #[command(name = "list-displays")]
    ListDisplays(ListDisplays),
    #[command(name = "hit-test")]
    HitTest(HitTest),
    #[command(name = "ocr")]
    Ocr(Ocr),
    Click(Click),
    Type(Type),
    #[command(name = "keyboard-type")]
    KeyboardType(KeyboardType),
    Press(Press),
    #[command(name = "ocr-click")]
    OcrClick(OcrClick),
    Hover(Hover),
    Wait(Wait),
    Scroll(Scroll),
    Drag(Drag),
    Batch(Batch),
    Permissions(Permissions),
}

fn main() -> anyhow::Result<()> {
    reset_sigpipe();

    forepaw::log::init();

    let app = App::parse();

    let provider = forepaw::provider();
    let globals = GlobalArgs::new(app.format, app.verbose);

    match app.command {
        Commands::Snapshot(cmd) => cmd.run(&*provider, globals),
        Commands::Screenshot(cmd) => cmd.run(&*provider, globals),
        Commands::ListApps(cmd) => cmd.run(&*provider, globals),
        Commands::ListWindows(cmd) => cmd.run(&*provider, globals),
        Commands::ListDisplays(cmd) => cmd.run(&*provider, globals),
        Commands::Ocr(cmd) => cmd.run(&*provider, globals),
        Commands::HitTest(cmd) => cmd.run(&*provider, globals),
        Commands::Click(cmd) => cmd.run(&*provider, globals),
        Commands::Type(cmd) => cmd.run(&*provider, globals),
        Commands::KeyboardType(cmd) => cmd.run(&*provider, globals),
        Commands::Press(cmd) => cmd.run(&*provider, globals),
        Commands::OcrClick(cmd) => cmd.run(&*provider, globals),
        Commands::Hover(cmd) => cmd.run(&*provider, globals),
        Commands::Wait(cmd) => cmd.run(&*provider, globals),
        Commands::Scroll(cmd) => cmd.run(&*provider, globals),
        Commands::Drag(cmd) => cmd.run(&*provider, globals),
        Commands::Batch(cmd) => cmd.run(&*provider, globals),
        Commands::Permissions(cmd) => cmd.run(&*provider, globals),
    }
}
