//! Basic usage example: create a provider and list running apps.
//!
//! Run with: `cargo run --example basic-usage -p forepaw`

use forepaw::platform::{AppTarget, SnapshotOptions};
use forepaw::provider;

fn main() {
    let provider = provider();

    // List running apps
    let apps = provider.list_apps().expect("should list running apps");
    println!("Running apps ({}):", apps.len());
    for app in &apps {
        println!("  {} (pid={})", app.name, app.pid);
    }

    // Snapshot an app's accessibility tree
    if let Some(app_name) = std::env::args().nth(1) {
        let tree = provider
            .snapshot(
                &AppTarget::name(&app_name),
                None,
                &SnapshotOptions::default(),
            )
            .expect("should snapshot app");
        println!("\nAccessibility tree for {app_name}:");
        println!("  {} refs assigned", tree.refs.len());
    }
}
