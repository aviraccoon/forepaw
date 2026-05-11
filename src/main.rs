/// Forepaw: desktop automation CLI for AI agents.
use clap::Parser;

use forepaw::cli::action::{
    Batch, Click, Drag, Hover, KeyboardType, OcrClick, Press, Scroll, Type, Wait,
};
use forepaw::cli::observation::{ListApps, ListWindows, Ocr, Screenshot, Snapshot};
use forepaw::cli::system::Permissions;
use forepaw::platform::DesktopProvider;

fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[derive(Parser)]
#[command(
    name = "forepaw",
    about = "A raccoon's paws on your desktop. Cross-platform automation CLI.",
    version = version(),
)]
struct App {
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
    let app = App::parse();

    #[cfg(target_os = "macos")]
    let provider = forepaw::platform::darwin::DarwinProvider::new();

    #[cfg(target_os = "windows")]
    let provider = forepaw::platform::windows::WindowsProvider::new();

    // Future: #[cfg(target_os = "linux")] let provider = LinuxProvider::new();

    let provider = &provider as &dyn DesktopProvider;

    match app.command {
        Commands::Snapshot(cmd) => cmd.run(provider),
        Commands::Screenshot(cmd) => cmd.run(provider),
        Commands::ListApps(cmd) => cmd.run(provider),
        Commands::ListWindows(cmd) => cmd.run(provider),
        Commands::Ocr(cmd) => cmd.run(provider),
        Commands::Click(cmd) => cmd.run(provider),
        Commands::Type(cmd) => cmd.run(provider),
        Commands::KeyboardType(cmd) => cmd.run(provider),
        Commands::Press(cmd) => cmd.run(provider),
        Commands::OcrClick(cmd) => cmd.run(provider),
        Commands::Hover(cmd) => cmd.run(provider),
        Commands::Wait(cmd) => cmd.run(provider),
        Commands::Scroll(cmd) => cmd.run(provider),
        Commands::Drag(cmd) => cmd.run(provider),
        Commands::Batch(cmd) => cmd.run(provider),
        Commands::Permissions(cmd) => cmd.run(provider),
    }
}
