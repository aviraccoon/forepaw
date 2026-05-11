/// Snapshot cache: saves/loads rendered snapshot text to temp files for diffing.
use std::fs;
use std::path::PathBuf;

pub struct SnapshotCache;

impl SnapshotCache {
    pub fn new() -> Self {
        Self
    }

    /// Save rendered snapshot text for an app.
    pub fn save(&self, app: &str, text: &str) -> std::io::Result<()> {
        let path = self.cache_path(app);
        fs::write(&path, text)
    }

    /// Load the last cached snapshot text for an app, if any.
    pub fn load(&self, app: &str) -> Option<String> {
        let path = self.cache_path(app);
        fs::read_to_string(path).ok()
    }

    /// Remove cached snapshot for an app.
    pub fn clear(&self, app: &str) {
        let path = self.cache_path(app);
        let _ = fs::remove_file(path);
    }

    fn cache_path(&self, app: &str) -> PathBuf {
        let sanitized = app.to_lowercase().replace(' ', "-");
        std::env::temp_dir().join(format!("forepaw-snapshot-{sanitized}.txt"))
    }
}

impl Default for SnapshotCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_roundtrip() {
        let cache = SnapshotCache::new();
        let app = format!("TestApp-{}", rand_random_suffix());
        let text = format!("app: {app}\n  button @e1 \"OK\"");

        cache.save(&app, &text).unwrap();
        let loaded = cache.load(&app);
        assert_eq!(loaded.as_deref(), Some(text.as_str()));
        cache.clear(&app);
        assert!(cache.load(&app).is_none());
    }

    #[test]
    fn cache_returns_nil_for_unknown() {
        let cache = SnapshotCache::new();
        assert!(cache.load("NonexistentApp-99999").is_none());
    }

    fn rand_random_suffix() -> u32 {
        // Simple non-crypto random for test namespacing
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u32
    }
}
