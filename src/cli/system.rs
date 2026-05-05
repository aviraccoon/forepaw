/// CLI subcommand: permissions check.
use crate::platform::DesktopProvider;

/// Check or request accessibility permissions.
#[derive(clap::Args)]
#[command(about = "Check or request accessibility permissions")]
pub struct Permissions {
    #[arg(long, help = "Prompt for permission")]
    pub request: bool,
}

impl Permissions {
    pub fn run(&self, provider: &dyn DesktopProvider) -> anyhow::Result<()> {
        let mut failed = false;

        let ax_help = "\n\
            To grant accessibility permission:\n\
              1. Open System Settings > Privacy & Security > Accessibility\n\
              2. Click the + button\n\
              3. Add your terminal app (Terminal, Ghostty, Warp, iTerm2, etc.)\n\
              4. Ensure the toggle is enabled";

        let sr_help = "\n\
            To grant screen recording permission:\n\
              1. Open System Settings > Privacy & Security > Screen & System Audio Recording\n\
              2. Click the + button\n\
              3. Add your terminal app\n\
              4. Ensure the toggle is enabled";

        if self.request {
            if provider.request_permissions() {
                println!("Accessibility: granted");
            } else {
                println!("Accessibility: not granted");
                println!("{}", ax_help);
                failed = true;
            }
            if provider.request_screen_recording_permission() {
                println!("Screen recording: granted");
            } else {
                println!("Screen recording: not granted");
                println!("{}", sr_help);
                failed = true;
            }
        } else {
            if provider.has_permissions() {
                println!("Accessibility: granted");
            } else {
                println!("Accessibility: not granted");
                println!("{}", ax_help);
                failed = true;
            }
            if provider.has_screen_recording_permission() {
                println!("Screen recording: granted");
            } else {
                println!("Screen recording: not granted");
                println!("{}", sr_help);
                failed = true;
            }
        }

        if failed {
            Err(anyhow::anyhow!("Permissions not fully granted"))
        } else {
            Ok(())
        }
    }
}
