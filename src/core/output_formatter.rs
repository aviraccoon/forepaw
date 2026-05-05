/// Output formatting: plain text or JSON.
///
/// Error with code, message, and optional suggestion.
#[derive(Debug, Clone)]
pub struct OutputError {
    pub code: &'static str,
    pub message: String,
    pub suggestion: Option<String>,
}

impl OutputError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            suggestion: None,
        }
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    pub const PERM_DENIED: &'static str = "PERM_DENIED";
    pub const APP_NOT_FOUND: &'static str = "APP_NOT_FOUND";
    pub const ELEMENT_NOT_FOUND: &'static str = "ELEMENT_NOT_FOUND";
    pub const STALE_REF: &'static str = "STALE_REF";
    pub const ACTION_FAILED: &'static str = "ACTION_FAILED";
    pub const INVALID_ARGS: &'static str = "INVALID_ARGS";
}

pub struct OutputFormatter {
    pub json: bool,
}

impl OutputFormatter {
    pub fn new(json: bool) -> Self {
        Self { json }
    }

    pub fn format(
        &self,
        success: bool,
        command: &str,
        data: &[(&str, &str)],
        error: Option<&OutputError>,
    ) -> String {
        if self.json {
            return self.format_json(success, command, data, error);
        }
        if let Some(err) = error {
            let mut out = format!("error: {}", err.message);
            if let Some(suggestion) = &err.suggestion {
                out = format!("{out}\nhint: {suggestion}");
            }
            return out;
        }
        // Look for "text" key in data
        if let Some((_, text)) = data.iter().find(|(k, _)| *k == "text") {
            return text.to_string();
        }
        if success {
            "ok".to_string()
        } else {
            "failed".to_string()
        }
    }

    fn format_json(
        &self,
        success: bool,
        command: &str,
        data: &[(&str, &str)],
        error: Option<&OutputError>,
    ) -> String {
        let mut pairs: Vec<String> = vec![
            format!("\"ok\": {success}"),
            format!("\"command\": \"{command}\""),
        ];
        if let Some(err) = error {
            let mut error_pairs = vec![
                format!("\"code\": \"{}\"", err.code),
                format!("\"message\": \"{}\"", escape_json(&err.message)),
            ];
            if let Some(suggestion) = &err.suggestion {
                error_pairs.push(format!("\"suggestion\": \"{}\"", escape_json(suggestion)));
            }
            pairs.push(format!("\"error\": {{{}}}", error_pairs.join(", ")));
        }
        for (key, val) in data {
            pairs.push(format!("\"{key}\": \"{}\"", escape_json(val)));
        }
        format!("{{{}}}", pairs.join(", "))
    }
}

fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_success_with_text() {
        let f = OutputFormatter::new(false);
        assert_eq!(
            f.format(true, "click", &[("text", "clicked")], None),
            "clicked"
        );
    }

    #[test]
    fn plain_text_success_no_data() {
        let f = OutputFormatter::new(false);
        assert_eq!(f.format(true, "click", &[], None), "ok");
    }

    #[test]
    fn plain_text_failure() {
        let f = OutputFormatter::new(false);
        assert_eq!(f.format(false, "click", &[], None), "failed");
    }

    #[test]
    fn plain_text_error_with_suggestion() {
        let f = OutputFormatter::new(false);
        let err =
            OutputError::new("STALE_REF", "Ref expired").with_suggestion("Run snapshot again");
        let output = f.format(false, "click", &[], Some(&err));
        assert!(output.contains("Ref expired"));
        assert!(output.contains("Run snapshot again"));
    }

    #[test]
    fn json_success() {
        let f = OutputFormatter::new(true);
        let output = f.format(true, "click", &[], None);
        assert!(output.contains("\"ok\": true"));
        assert!(output.contains("\"command\": \"click\""));
    }

    #[test]
    fn json_error() {
        let f = OutputFormatter::new(true);
        let err = OutputError::new("APP_NOT_FOUND", "No such app");
        let output = f.format(false, "click", &[], Some(&err));
        assert!(output.contains("\"ok\": false"));
        assert!(output.contains("\"code\": \"APP_NOT_FOUND\""));
        assert!(output.contains("\"message\": \"No such app\""));
    }

    #[test]
    fn json_escapes_special() {
        let f = OutputFormatter::new(true);
        let err = OutputError::new("ERR", "line1\nline2\twith \"quotes\"");
        let output = f.format(false, "test", &[], Some(&err));
        assert!(output.contains("\\n"));
        assert!(output.contains("\\t"));
        assert!(output.contains("\\\"quotes\\\""));
    }
}
