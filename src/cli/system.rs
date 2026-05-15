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

        let binary = std::env::current_exe().unwrap_or_default();
        let binary_display = binary.display();

        if self.request {
            if provider.request_permissions() {
                println!("Accessibility: granted");
            } else {
                println!("Accessibility: not granted");
                print_add_help("Accessibility", &binary_display);
                failed = true;
            }
            if provider.request_screen_recording_permission() {
                // CGRequestScreenCaptureAccess opens System Settings.
                // But the API can report success while data is still redacted
                // (new binary, cache lag, etc). Validate by checking actual
                // window data.
                let sr_validated = provider.validate_screen_recording();
                if sr_validated {
                    println!("Screen recording: granted");
                } else {
                    println!("Screen recording: API reports granted, but window data is redacted");
                    println!("  The binary may need to be added to System Settings manually:");
                    println!("  {binary_display}");
                    print_add_help("Screen & System Audio Recording", &binary_display);
                    failed = true;
                }
            } else {
                println!("Screen recording: not granted");
                print_add_help("Screen & System Audio Recording", &binary_display);
                failed = true;
            }
        } else {
            if provider.has_permissions() {
                println!("Accessibility: granted");
            } else {
                println!("Accessibility: not granted");
                print_add_help("Accessibility", &binary_display);
                failed = true;
            }
            if provider.has_screen_recording_permission() {
                let sr_validated = provider.validate_screen_recording();
                if sr_validated {
                    println!("Screen recording: granted");
                } else {
                    println!("Screen recording: API reports granted, but window data is redacted");
                    println!("  Binary: {binary_display}");
                    print_add_help("Screen & System Audio Recording", &binary_display);
                    failed = true;
                }
            } else {
                println!("Screen recording: not granted");
                print_add_help("Screen & System Audio Recording", &binary_display);
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

fn print_add_help(section: &str, binary: &std::path::Display<'_>) {
    println!(
        "\n\
          Add this binary to System Settings:\n\
          1. Open System Settings > Privacy & Security > {section}\n\
          2. Click the + button\n\
          3. Navigate to and select:\n\
             {binary}\n\
          4. Ensure the toggle is enabled"
    );
}
