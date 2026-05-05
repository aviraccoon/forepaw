/// Forepaw: desktop automation CLI for AI agents.
///
/// Crab-eating raccoon edition (Rust rewrite).
use clap::Parser;

use forepaw::cli::action::{
    Batch, Click, Drag, Hover, KeyboardType, OcrClick, Press, Scroll, Type, Wait,
};
use forepaw::cli::observation::{ListApps, ListWindows, Ocr, Screenshot, Snapshot};
use forepaw::cli::system::Permissions;

/// Base version. Updated at release time.
const BASE_VERSION: &str = "0.3.0";

fn version() -> &'static str {
    BASE_VERSION
}

#[derive(Parser)]
#[command(
    name = "forepaw",
    about = "A raccoon's paws on your UI. Desktop automation for AI agents.",
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
    let _app = App::parse();

    // For now, we have no platform backend. Each command would need a provider.
    // The platform module will provide a factory function once a backend exists.
    eprintln!("forepaw cancrivorus -- CLI structure wired, no platform backend yet");
    std::process::exit(1);

    // Once a platform backend exists:
    // let provider = forepaw::platform::create_provider();
    // match _app.command {
    //     Commands::Snapshot(cmd) => cmd.run(provider.as_ref()),
    //     ...
    // }
}
