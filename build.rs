use std::process::Command;

/// Embed the git commit SHA into the binary at build time.
///
/// Sets `FOREPAW_GIT_SHA` env var for use via `env!()` in main.rs.
/// Falls back to "unknown" when git is unavailable (e.g. Nix sandbox).
/// Appends "-dirty" when the working tree has any changes.
fn main() {
    let sha = git_short_sha().unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=FOREPAW_GIT_SHA={sha}");
    println!("cargo:rerun-if-changed=.git/HEAD");
    // Also watch the ref that HEAD points to, so commit changes trigger rebuild
    if let Ok(head) = std::fs::read_to_string(".git/HEAD") {
        let ref_path = head.trim().strip_prefix("ref: ").unwrap_or("");
        if !ref_path.is_empty() {
            println!("cargo:rerun-if-changed=.git/{ref_path}");
        }
    }
}

fn git_short_sha() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let sha = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if sha.is_empty() {
        return None;
    }

    // Check dirty: staged changes, unstaged changes, or untracked files
    let dirty = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .is_none_or(|o| !o.stdout.is_empty());

    Some(if dirty { format!("{sha}-dirty") } else { sha })
}
