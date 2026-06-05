//! Image format and encoder detection.

/// Image format for screenshots.
///
/// Used by screenshot operations and encoder detection.
/// `BestAvailable` is a sentinel that resolves to the best concrete format
/// at screenshot time (WebP if `cwebp` is installed, otherwise JPEG).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ImageFormat {
    /// PNG (lossless).
    Png,
    /// JPEG (lossy, configurable quality).
    Jpeg,
    /// WebP (lossy or lossless).
    Webp,
    /// Best available format for the platform (JPEG fallback).
    BestAvailable,
}

impl ImageFormat {
    /// If `BestAvailable`, resolve to the best concrete format. Otherwise, pass through.
    #[must_use]
    pub fn resolve(&self) -> Self {
        #[expect(
            clippy::wildcard_enum_match_arm,
            reason = "new variants should resolve to themselves"
        )]
        match self {
            Self::BestAvailable => Self::best_available_concrete(),
            other => *other,
        }
    }

    /// Return the file extension for this format.
    /// For `BestAvailable`, resolves first so the extension matches the
    /// actual format that will be used.
    #[must_use]
    pub fn file_extension(&self) -> &'static str {
        self.resolve().file_extension_inner()
    }

    fn file_extension_inner(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
            Self::BestAvailable => {
                unreachable!("file_extension_inner called on BestAvailable — use resolve() first")
            }
        }
    }

    /// Return all concrete (non-sentinel) formats.
    #[must_use]
    pub fn all() -> &'static [Self] {
        &[Self::Png, Self::Jpeg, Self::Webp]
    }

    /// Best available concrete format: WebP if cwebp installed, else JPEG.
    #[must_use]
    pub fn best_available_concrete() -> Self {
        if is_command_available("cwebp") {
            Self::Webp
        } else {
            Self::Jpeg
        }
    }
}

impl std::str::FromStr for ImageFormat {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "png" => Ok(Self::Png),
            "jpeg" | "jpg" => Ok(Self::Jpeg),
            "webp" => Ok(Self::Webp),
            _ => Err(()),
        }
    }
}

/// Screenshot output options.
#[derive(Debug, Clone)]
pub struct ScreenshotOptions {
    /// Output image format.
    pub format: ImageFormat,
    /// JPEG/WebP quality (1-100).
    pub quality: u32,
    /// Upscale factor.
    pub scale: u32,
    /// Whether to include the cursor.
    pub cursor: bool,
}

impl Default for ScreenshotOptions {
    fn default() -> Self {
        Self {
            format: ImageFormat::BestAvailable,
            quality: 85,
            scale: 1,
            cursor: true,
        }
    }
}

impl ScreenshotOptions {
    /// Full quality: PNG, 2x Retina, cursor visible.
    #[must_use]
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
#[must_use]
pub fn is_command_available(command: &str) -> bool {
    #[cfg(unix)]
    {
        std::process::Command::new("/usr/bin/env")
            .args(["which", command])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }
    #[cfg(windows)]
    {
        std::process::Command::new("where")
            .arg(command)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_from_str() {
        assert_eq!("png".parse(), Ok(ImageFormat::Png));
        assert_eq!("jpeg".parse(), Ok(ImageFormat::Jpeg));
        assert_eq!("webp".parse(), Ok(ImageFormat::Webp));
        assert_eq!("gif".parse::<ImageFormat>(), Err(()));
    }

    #[test]
    fn file_extensions() {
        assert_eq!(ImageFormat::Png.file_extension(), "png");
        assert_eq!(ImageFormat::Jpeg.file_extension(), "jpg");
        assert_eq!(ImageFormat::Webp.file_extension(), "webp");
    }

    #[test]
    fn best_available_concrete_is_jpeg_or_webp() {
        let best = ImageFormat::best_available_concrete();
        assert!(best == ImageFormat::Jpeg || best == ImageFormat::Webp);
    }

    #[test]
    fn best_available_resolves_correctly() {
        let resolved = ImageFormat::BestAvailable.resolve();
        assert!(
            resolved == ImageFormat::Jpeg || resolved == ImageFormat::Webp,
            "BestAvailable should resolve to Jpeg or Webp, got {resolved:?}"
        );
        // file_extension after resolve matches the concrete format
        assert_eq!(
            resolved.file_extension(),
            ImageFormat::best_available_concrete().file_extension()
        );
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
        // Nix build sandbox has a minimal PATH without standard tools.
        if std::env::var("NIX_BUILD_TOP").is_ok() {
            return;
        }
        // Use a command that exists on the current platform.
        let cmd = if cfg!(windows) { "cmd" } else { "env" };
        assert!(is_command_available(cmd));
    }

    #[test]
    fn missing_tool() {
        assert!(!is_command_available("forepaw-nonexistent-tool-xyz"));
    }
}
