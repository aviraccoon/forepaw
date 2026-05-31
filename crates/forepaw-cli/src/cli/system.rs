/// CLI subcommand: permissions check.
use crate::cli::GlobalArgs;
use forepaw::platform::DesktopProvider;

/// Check or request accessibility permissions.
#[derive(clap::Args)]
#[command(about = "Check or request accessibility permissions")]
pub(crate) struct Permissions {
    #[arg(long, help = "Prompt for permission")]
    pub request: bool,
}

impl Permissions {
    /// Checks and reports accessibility and screen recording permissions.
    ///
    /// # Errors
    ///
    /// Returns an error if the current executable path cannot be determined.
    pub(crate) fn run(
        &self,
        provider: &dyn DesktopProvider,
        globals: GlobalArgs,
    ) -> anyhow::Result<()> {
        let binary = std::env::current_exe().unwrap_or_default();
        let binary_display = binary.display();

        // Check/request permissions
        let (ax_granted, sr_granted, sr_validated) = if self.request {
            let ax = provider.request_permissions();
            let sr_api = provider.request_screen_recording_permission();
            // CGRequestScreenCaptureAccess opens System Settings.
            // The API can report success while data is still redacted
            // (new binary, cache lag, etc). Validate by checking actual
            // window data.
            let sr_valid = sr_api && provider.validate_screen_recording();
            (ax, sr_api, sr_valid)
        } else {
            let ax = provider.has_permissions();
            let sr_api = provider.has_screen_recording_permission();
            let sr_valid = sr_api && provider.validate_screen_recording();
            (ax, sr_api, sr_valid)
        };

        if globals.json() {
            #[derive(serde::Serialize)]
            struct PermissionsResult {
                accessibility: &'static str,
                screen_recording: &'static str,
                screen_recording_validated: bool,
                #[serde(skip_serializing_if = "Option::is_none")]
                binary: Option<String>,
            }
            let result = PermissionsResult {
                accessibility: if ax_granted { "granted" } else { "not_granted" },
                screen_recording: if sr_granted {
                    if sr_validated {
                        "granted"
                    } else {
                        "not_validated"
                    }
                } else {
                    "not_granted"
                },
                screen_recording_validated: sr_validated,
                binary: if !ax_granted || !sr_validated {
                    Some(binary_display.to_string())
                } else {
                    None
                },
            };
            println!(
                "{}",
                serde_json::to_string(&result).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
            );
        } else {
            let mut failed = false;
            if ax_granted {
                println!("Accessibility: granted");
            } else {
                println!("Accessibility: not granted");
                print_add_help("Accessibility", &binary_display);
                failed = true;
            }
            if sr_validated {
                println!("Screen recording: granted");
            } else if sr_granted {
                println!("Screen recording: API reports granted, but window data is redacted");
                println!("  Binary: {binary_display}");
                print_add_help("Screen & System Audio Recording", &binary_display);
                failed = true;
            } else {
                println!("Screen recording: not granted");
                print_add_help("Screen & System Audio Recording", &binary_display);
                failed = true;
            }
            if failed {
                return Err(anyhow::anyhow!("Permissions not fully granted"));
            }
        }

        Ok(())
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
