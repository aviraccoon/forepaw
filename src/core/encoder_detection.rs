/// Image format and encoder detection.

/// Image format for screenshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Webp,
}

impl ImageFormat {
    pub fn file_extension(&self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "png" => Some(Self::Png),
            "jpeg" | "jpg" => Some(Self::Jpeg),
            "webp" => Some(Self::Webp),
            _ => None,
        }
    }

    pub fn all() -> &'static [ImageFormat] {
        &[Self::Png, Self::Jpeg, Self::Webp]
    }

    /// Best available format: WebP if cwebp installed, else JPEG.
    pub fn best_available() -> Self {
        if is_command_available("cwebp") {
            Self::Webp
        } else {
            Self::Jpeg
        }
    }
}

/// Screenshot output options.
#[derive(Debug, Clone)]
pub struct ScreenshotOptions {
    pub format: ImageFormat,
    pub quality: u32,
    pub scale: u32,
    pub cursor: bool,
}

impl Default for ScreenshotOptions {
    fn default() -> Self {
        Self {
            format: ImageFormat::best_available(),
            quality: 85,
            scale: 1,
            cursor: true,
        }
    }
}

impl ScreenshotOptions {
    /// Full quality: PNG, 2x Retina, cursor visible.
    pub fn full_quality() -> Self {
        Self {
            format: ImageFormat::Png,
            quality: 85,
            scale: 2,
            cursor: true,
        }
    }
}

/// Check whether a command-line tool is available in PATH.
pub fn is_command_available(command: &str) -> bool {
    std::process::Command::new("/usr/bin/env")
        .args(["which", command])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_from_str() {
        assert_eq!(ImageFormat::from_str("png"), Some(ImageFormat::Png));
        assert_eq!(ImageFormat::from_str("jpeg"), Some(ImageFormat::Jpeg));
        assert_eq!(ImageFormat::from_str("webp"), Some(ImageFormat::Webp));
        assert_eq!(ImageFormat::from_str("gif"), None);
    }

    #[test]
    fn file_extensions() {
        assert_eq!(ImageFormat::Png.file_extension(), "png");
        assert_eq!(ImageFormat::Jpeg.file_extension(), "jpg");
        assert_eq!(ImageFormat::Webp.file_extension(), "webp");
    }

    #[test]
    fn best_available_is_jpeg_or_webp() {
        let best = ImageFormat::best_available();
        assert!(best == ImageFormat::Jpeg || best == ImageFormat::Webp);
    }

    #[test]
    fn default_options() {
        let opts = ScreenshotOptions::default();
        assert_eq!(opts.quality, 85);
        assert_eq!(opts.scale, 1);
        assert!(opts.cursor);
    }

    #[test]
    fn full_quality() {
        let opts = ScreenshotOptions::full_quality();
        assert_eq!(opts.format, ImageFormat::Png);
        assert_eq!(opts.scale, 2);
    }

    #[test]
    fn finds_system_tools() {
        assert!(is_command_available("env"));
    }

    #[test]
    fn missing_tool() {
        assert!(!is_command_available("forepaw-nonexistent-tool-xyz"));
    }
}
