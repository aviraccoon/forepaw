/// Forepaw: desktop automation CLI for AI agents.
use clap::Parser;

use forepaw::cli::action::{
    Batch, Click, Drag, Hover, KeyboardType, OcrClick, Press, Scroll, Type, Wait,
};
use forepaw::cli::observation::{HitTest, ListApps, ListWindows, Ocr, Screenshot, Snapshot};
use forepaw::cli::system::Permissions;
use forepaw::cli::GlobalArgs;
use forepaw::core::output_formatter::OutputFormat;
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
    forepaw::log::init();

    let app = App::parse();

    #[cfg(target_os = "macos")]
    let provider = forepaw::platform::darwin::DarwinProvider::new();

    #[cfg(target_os = "windows")]
    let provider = forepaw::platform::windows::WindowsProvider::new();

    #[cfg(target_os = "linux")]
    let provider = forepaw::platform::linux::LinuxProvider::new();

    let provider = &provider as &dyn DesktopProvider;
    let globals = GlobalArgs::new(app.format, app.verbose);

    match app.command {
        Commands::Snapshot(cmd) => cmd.run(provider, &globals),
        Commands::Screenshot(cmd) => cmd.run(provider, &globals),
        Commands::ListApps(cmd) => cmd.run(provider, &globals),
        Commands::ListWindows(cmd) => cmd.run(provider, &globals),
        Commands::Ocr(cmd) => cmd.run(provider, &globals),
        Commands::HitTest(cmd) => cmd.run(provider, &globals),
        Commands::Click(cmd) => cmd.run(provider, &globals),
        Commands::Type(cmd) => cmd.run(provider, &globals),
        Commands::KeyboardType(cmd) => cmd.run(provider, &globals),
        Commands::Press(cmd) => cmd.run(provider, &globals),
        Commands::OcrClick(cmd) => cmd.run(provider, &globals),
        Commands::Hover(cmd) => cmd.run(provider, &globals),
        Commands::Wait(cmd) => cmd.run(provider, &globals),
        Commands::Scroll(cmd) => cmd.run(provider, &globals),
        Commands::Drag(cmd) => cmd.run(provider, &globals),
        Commands::Batch(cmd) => cmd.run(provider, &globals),
        Commands::Permissions(cmd) => cmd.run(provider, &globals),
    }
}
