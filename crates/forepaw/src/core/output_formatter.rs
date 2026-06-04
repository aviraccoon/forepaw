//! Output formatting: plain text or JSON.
/// Output formatting: plain text or JSON.
///
/// Error with code, message, and optional suggestion.
#[derive(Debug, Clone, serde::Serialize)]
#[must_use]
pub struct OutputError {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
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

/// Output format for CLI commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    /// Human-readable text (default).
    #[default]
    Text,
    /// Machine-readable JSON.
    Json,
}

impl std::str::FromStr for OutputFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "text" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            _ => Err(format!("unknown format: {s} (expected: text, json)")),
        }
    }
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text => write!(f, "text"),
            Self::Json => write!(f, "json"),
        }
    }
}

pub struct OutputFormatter {
    pub format: OutputFormat,
}

impl OutputFormatter {
    #[must_use]
    pub fn new(format: OutputFormat) -> Self {
        Self { format }
    }

    #[must_use]
    pub fn format(
        &self,
        success: bool,
        command: &str,
        data: &[(&str, &str)],
        error: Option<&OutputError>,
    ) -> String {
        if self.format == OutputFormat::Json {
            return Self::format_json(success, command, data, error);
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
            "ok".to_owned()
        } else {
            "failed".to_owned()
        }
    }

    fn format_json(
        success: bool,
        command: &str,
        data: &[(&str, &str)],
        error: Option<&OutputError>,
    ) -> String {
        #[derive(serde::Serialize)]
        struct Output<'a> {
            ok: bool,
            command: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            error: Option<&'a OutputError>,
            #[serde(flatten)]
            data: std::collections::BTreeMap<&'a str, &'a str>,
        }

        let data_map: std::collections::BTreeMap<&str, &str> =
            data.iter().map(|(k, v)| (*k, *v)).collect();

        let output = Output {
            ok: success,
            command,
            error,
            data: data_map,
        };

        serde_json::to_string(&output).unwrap_or_else(|e| {
            format!(
                "{{\"ok\":false,\"error\":{{\"code\":\"SERIALIZE_ERROR\",\"message\":\"{e}\"}}}}"
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_success_with_text() {
        let f = OutputFormatter::new(OutputFormat::Text);
        assert_eq!(
            f.format(true, "click", &[("text", "clicked")], None),
            "clicked"
        );
    }

    #[test]
    fn plain_text_success_no_data() {
        let f = OutputFormatter::new(OutputFormat::Text);
        assert_eq!(f.format(true, "click", &[], None), "ok");
    }

    #[test]
    fn plain_text_failure() {
        let f = OutputFormatter::new(OutputFormat::Text);
        assert_eq!(f.format(false, "click", &[], None), "failed");
    }

    #[test]
    fn plain_text_error_with_suggestion() {
        let f = OutputFormatter::new(OutputFormat::Text);
        let err =
            OutputError::new("STALE_REF", "Ref expired").with_suggestion("Run snapshot again");
        let output = f.format(false, "click", &[], Some(&err));
        assert!(output.contains("Ref expired"));
        assert!(output.contains("Run snapshot again"));
    }

    #[test]
    fn json_success() {
        let f = OutputFormatter::new(OutputFormat::Json);
        let output = f.format(true, "click", &[], None);
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["ok"], true);
        assert_eq!(parsed["command"], "click");
    }

    #[test]
    fn json_error() {
        let f = OutputFormatter::new(OutputFormat::Json);
        let err = OutputError::new("APP_NOT_FOUND", "No such app");
        let output = f.format(false, "click", &[], Some(&err));
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["ok"], false);
        assert_eq!(parsed["error"]["code"], "APP_NOT_FOUND");
        assert_eq!(parsed["error"]["message"], "No such app");
    }

    #[test]
    fn json_escapes_special() {
        let f = OutputFormatter::new(OutputFormat::Json);
        let err = OutputError::new("ERR", "line1\nline2\twith \"quotes\"");
        let output = f.format(false, "test", &[], Some(&err));
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["error"]["message"], "line1\nline2\twith \"quotes\"");
    }

    #[test]
    fn format_enum_from_str() {
        assert_eq!("text".parse::<OutputFormat>(), Ok(OutputFormat::Text));
        assert_eq!("json".parse::<OutputFormat>(), Ok(OutputFormat::Json));
        assert!("xml".parse::<OutputFormat>().is_err());
    }

    #[test]
    fn format_enum_default() {
        assert_eq!(OutputFormat::default(), OutputFormat::Text);
    }
}
