/// CLI subcommands: observation (snapshot, screenshot, list-apps, list-windows, ocr).
use clap::Args;

use crate::cli::parse::{parse_coordinate, parse_region};
use crate::core::annotation::AnnotationStyle;
use crate::core::crop_region::CropRegion;
use crate::core::element_tree::ElementRef;
use crate::core::snapshot_cache::SnapshotCache;
use crate::core::snapshot_diff::SnapshotDiffer;
use crate::core::tree_renderer::TreeRenderer;
use crate::platform::{DesktopProvider, ImageFormat, ScreenshotOptions, SnapshotOptions};

/// Maximum length for a displayed value or name in hit-test output.
/// Terminal content, web page text, and large text fields can be
/// hundreds of KB — never useful to dump inline.
const HIT_DISPLAY_MAX: usize = 200;

/// Truncate a string for inline display, appending a note if truncated.
fn truncate_display(s: &str) -> String {
    let display: String = s.chars().take(HIT_DISPLAY_MAX).collect();
    if s.len() > HIT_DISPLAY_MAX {
        format!("{}[... {} more chars]", display, s.len() - HIT_DISPLAY_MAX)
    } else {
        display
    }
}

/// Shared global options (app/pid, window, json).
#[derive(Args, Clone)]
pub struct GlobalOptions {
    #[command(flatten)]
    pub app_target: crate::cli::AppTargetArgs,

    #[command(flatten)]
    pub window_target: crate::cli::WindowTargetArgs,

    #[arg(long, help = "JSON output")]
    pub json: bool,
}

/// Accessibility tree with element refs.
#[derive(clap::Args)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "CLI flags accumulate booleans"
)]
#[command(about = "Accessibility tree with element refs")]
pub struct Snapshot {
    #[command(flatten)]
    pub global: GlobalOptions,

    #[arg(short, long, help = "Only interactive elements")]
    pub interactive: bool,

    #[arg(short, long, help = "Remove empty structural elements")]
    pub compact: bool,

    #[arg(short, long, help = "Maximum tree depth (default 15)")]
    pub depth: Option<usize>,

    #[arg(long, help = "Show diff against previous snapshot of this app")]
    pub diff: bool,

    #[arg(
        long,
        help = "Context lines around changes in diff output (default: 0)",
        default_value = "0"
    )]
    pub context: usize,

    #[arg(long, help = "Include menu bar (excluded by default with -i)")]
    pub menu: bool,

    #[arg(
        long,
        help = "Include zero-size elements (excluded by default with -i)"
    )]
    pub zero_size: bool,

    #[arg(long, help = "Include offscreen elements (excluded by default)")]
    pub offscreen: bool,

    #[arg(long, help = "Show per-subtree timing breakdown on stderr")]
    pub timing: bool,
}

impl Snapshot {
    /// Walks the accessibility tree and prints it, optionally diffing against a cached snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error if `--app` is missing, the application is not running,
    /// or accessibility permission is denied.
    pub fn run(&self, provider: &dyn DesktopProvider) -> anyhow::Result<()> {
        let app = self
            .global
            .app_target
            .resolve()?
            .ok_or_else(|| anyhow::anyhow!("--app or --pid is required"))?;

        let interactive = self.interactive;
        let depth = self.depth.unwrap_or(SnapshotOptions::DEFAULT_DEPTH);
        let include_hidden = self.zero_size || self.menu;

        let options = SnapshotOptions {
            interactive_only: interactive,
            max_depth: depth,
            compact: self.compact,
            skip_menu_bar: interactive && !self.menu,
            skip_zero_size: interactive && !include_hidden,
            skip_offscreen: !self.offscreen,
            timing: self.timing,
            ..Default::default()
        };

        let window_target = self.global.window_target.resolve();

        let tree = provider.snapshot(&app, window_target.as_ref(), &options)?;
        let tree_renderer = TreeRenderer::new();
        let rendered = tree_renderer.render(&tree);

        let cache = SnapshotCache::new();
        let cache_key = app.cache_key();

        if self.diff {
            if let Some(previous) = cache.load(&cache_key) {
                let differ = SnapshotDiffer::new();
                let result = differ.diff(&previous, &rendered);
                println!("{}", result.render(self.context));
            } else {
                println!("[diff: no previous snapshot cached for {cache_key}]");
                println!("{rendered}");
            }
        } else {
            println!("{rendered}");
        }

        if let Some(ref timing_data) = tree.timing {
            let report = timing_data.report();
            eprintln!("{report}");
        }

        // Always cache for future diffs
        cache.save(&cache_key, &rendered).ok();

        Ok(())
    }
}

/// Take a screenshot.
#[derive(clap::Args)]
#[command(about = "Take a screenshot")]
pub struct Screenshot {
    #[command(flatten)]
    pub global: GlobalOptions,

    #[arg(
        long,
        help = "Annotate with numbered labels (shorthand for --style badges)"
    )]
    pub annotate: bool,

    #[arg(long, help = "Annotation style: badges, labeled, spotlight")]
    pub style: Option<String>,

    #[arg(long, num_args = 1.., help = "Only annotate these refs (e.g. --only @e5 @e8)")]
    pub only: Vec<String>,

    #[arg(long, help = "Image format: jpeg, png, or webp")]
    pub format: Option<String>,

    #[arg(long, help = "JPEG quality 1-100 (default 85)")]
    pub quality: Option<u32>,

    #[arg(long, help = "Output scale: 1 or 2 (default 1)")]
    pub scale: Option<u32>,

    #[arg(long, help = "Exclude mouse cursor from screenshot")]
    pub no_cursor: bool,

    #[arg(
        long,
        help = "Crop to element ref bounds (e.g. --ref @e5). Requires --app."
    )]
    pub r#ref: Option<String>,

    #[arg(long, help = "Crop to region: x,y,w,h. Requires --app.")]
    pub region: Option<String>,

    #[arg(long, help = "Padding around crop in logical pixels (default 20)")]
    pub padding: Option<f64>,

    #[arg(long, help = "Overlay coordinate grid with spacing (e.g. --grid 50)")]
    pub grid: Option<u32>,
}

impl Screenshot {
    /// Captures a screenshot, optionally annotated with element labels.
    ///
    /// # Errors
    ///
    /// Returns an error if `--ref` is given without `--app`, the ref is invalid,
    /// or the provider fails to capture (permission denied, app not found).
    pub fn run(&self, provider: &dyn DesktopProvider) -> anyhow::Result<()> {
        let annotation_style = self.resolve_annotation_style();
        let ref_filter: Option<Vec<ElementRef>> = if self.only.is_empty() {
            None
        } else {
            Some(
                self.only
                    .iter()
                    .filter_map(|s| ElementRef::parse(s))
                    .collect(),
            )
        };

        let ss_options = self.build_screenshot_options();
        let crop_region = self.resolve_crop_region(provider)?;
        let app_target = self.global.app_target.resolve()?;
        let window_target = self.global.window_target.resolve();

        let params = crate::platform::ScreenshotParams {
            app: app_target.as_ref(),
            window: window_target.as_ref(),
            style: annotation_style,
            only: ref_filter.as_deref(),
            options: &ss_options,
            crop: crop_region.as_ref(),
            grid_spacing: self.grid,
        };
        let result = provider.screenshot(&params)?;

        println!("{}", result.path);
        if let Some(legend) = &result.legend {
            println!("{legend}");
        }

        Ok(())
    }

    fn resolve_annotation_style(&self) -> Option<AnnotationStyle> {
        if let Some(ref style) = self.style {
            style.parse().ok()
        } else if self.annotate {
            Some(AnnotationStyle::Badges)
        } else {
            None
        }
    }

    fn build_screenshot_options(&self) -> ScreenshotOptions {
        let fmt = self
            .format
            .as_deref()
            .and_then(|s| s.parse().ok())
            .unwrap_or(ImageFormat::BestAvailable);

        ScreenshotOptions {
            format: fmt,
            quality: self.quality.unwrap_or(85),
            scale: self.scale.unwrap_or(1),
            cursor: !self.no_cursor,
        }
    }

    fn resolve_crop_region(
        &self,
        provider: &dyn DesktopProvider,
    ) -> anyhow::Result<Option<CropRegion>> {
        let pad = self.padding.unwrap_or(20.0);

        if let Some(ref ref_str) = self.r#ref {
            let app = self
                .global
                .app_target
                .resolve()?
                .ok_or_else(|| anyhow::anyhow!("--app or --pid is required"))?;
            let element_ref = ElementRef::parse(ref_str)
                .ok_or_else(|| anyhow::anyhow!("Invalid ref format: {ref_str}. Expected @eN"))?;
            let bounds = provider.resolve_ref_bounds(element_ref, &app)?;
            return Ok(Some(CropRegion::new(bounds, pad)));
        }

        if let Some(ref region) = self.region {
            let rect = parse_region(region).ok_or_else(|| {
                anyhow::anyhow!("Invalid region format: {region}. Expected x,y,w,h")
            })?;
            return Ok(Some(CropRegion::new(rect, pad)));
        }

        Ok(None)
    }
}

/// List running GUI applications.
#[derive(clap::Args)]
#[command(about = "List running GUI applications")]
pub struct ListApps {
    #[arg(long, help = "JSON output")]
    pub json: bool,
}

impl ListApps {
    /// Lists running GUI applications.
    ///
    /// # Errors
    ///
    /// Returns an error if accessibility permission is denied.
    pub fn run(&self, provider: &dyn DesktopProvider) -> anyhow::Result<()> {
        let apps = provider.list_apps()?;
        if self.json {
            // Simple JSON output
            print!("[");
            let items: Vec<String> = apps
                .iter()
                .map(|a| {
                    let bundle = a
                        .bundle_id
                        .as_deref()
                        .map(|b| format!(", \"bundleID\": \"{b}\""))
                        .unwrap_or_default();
                    format!("{{\"name\": \"{}\"{}, \"pid\": {}}}", a.name, bundle, a.pid)
                })
                .collect();
            println!("{}]", items.join(", "));
        } else {
            let mut sorted = apps;
            sorted.sort_by(|a, b| a.name.cmp(&b.name));
            for app in sorted {
                let bundle = app
                    .bundle_id
                    .as_deref()
                    .map(|b| format!(" ({b})"))
                    .unwrap_or_default();
                println!("{}{} [pid: {}]", app.name, bundle, app.pid);
            }
        }
        Ok(())
    }
}

/// List visible windows.
#[derive(clap::Args)]
#[command(about = "List visible windows")]
pub struct ListWindows {
    #[command(flatten)]
    pub global: GlobalOptions,
}

impl ListWindows {
    /// Lists visible windows, optionally filtered by application.
    ///
    /// # Errors
    ///
    /// Returns an error if the specified application is not found.
    pub fn run(&self, provider: &dyn DesktopProvider) -> anyhow::Result<()> {
        let app_target = self.global.app_target.resolve()?;
        let windows = provider.list_windows(app_target.as_ref())?;
        for w in windows {
            println!("{}  {}  \"{}\"", w.id, w.app, w.title);
        }
        Ok(())
    }
}

/// Hit-test an accessibility element at screen coordinates.
#[derive(clap::Args)]
#[command(about = "Find what element is at screen coordinates")]
pub struct HitTest {
    #[command(flatten)]
    pub global: GlobalOptions,

    /// Coordinates as "x,y" (screen coordinates).
    pub point: String,

    /// Show full element values without truncation (default truncates at 200 chars).
    #[arg(long)]
    pub full_values: bool,
}

impl HitTest {
    /// Performs a hit test at the given screen coordinates and prints the result.
    ///
    /// # Errors
    ///
    /// Returns an error if the coordinates are invalid, or the hit test fails
    /// (no element at point, permission denied, etc).
    pub fn run(&self, provider: &dyn DesktopProvider) -> anyhow::Result<()> {
        let point = parse_coordinate(&self.point)
            .ok_or_else(|| anyhow::anyhow!("Invalid coordinates: {}. Expected x,y", self.point))?;

        let app_target = self.global.app_target.resolve()?;
        let result = provider.element_at_point(point, app_target.as_ref())?;

        let truncate = |s: &str| {
            if self.full_values {
                s.to_owned()
            } else {
                truncate_display(s)
            }
        };

        let value_display = result.value.as_ref().map(|v| truncate(v));

        if self.global.json {
            let name = truncate(result.name.as_deref().unwrap_or(""));
            let value = truncate(result.value.as_deref().unwrap_or(""));
            let (bx, by, bw, bh) = result
                .bounds
                .map_or((0.0, 0.0, 0.0, 0.0), |b| (b.x, b.y, b.width, b.height));
            println!(
                "{{ \"role\": \"{}\", \"name\": \"{}\", \"value\": \"{}\", \"bounds\": [{:.0}, {:.0}, {:.0}, {:.0}], \"pid\": {}, \"actions\": [{}], \"ancestors\": [{}] }}",
                result.role.short_name(),
                name.escape_default(),
                value.escape_default(),
                bx, by, bw, bh,
                result.pid,
                result
                    .actions
                    .iter()
                    .map(|a| format!("\"{a}\""))
                    .collect::<Vec<_>>()
                    .join(", "),
                result
                    .ancestors
                    .iter()
                    .map(|a| {
                        let an = truncate(a.name.as_deref().unwrap_or(""));
                        format!("{{ \"role\": \"{}\", \"name\": \"{}\" }}", a.role.short_name(), an.escape_default())
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            return Ok(());
        }

        println!("Element at ({:.0}, {:.0}):", point.x, point.y);
        println!("  Role:      {}", result.role.short_name());
        if let Some(ref name) = result.name {
            println!("  Name:      {}", truncate(name));
        }
        if let Some(ref value) = value_display {
            println!("  Value:     {value}");
        }
        if let Some(ref bounds) = result.bounds {
            println!(
                "  Bounds:    [{:.0}, {:.0}, {:.0}, {:.0}]",
                bounds.x, bounds.y, bounds.width, bounds.height
            );
        }
        if !result.actions.is_empty() {
            println!("  Actions:   {}", result.actions.join(", "));
        }
        if result.pid > 0 {
            println!("  PID:       {}", result.pid);
        }

        if !result.ancestors.is_empty() {
            println!("  Ancestors:");
            for (i, ancestor) in result.ancestors.iter().enumerate() {
                let label = ancestor
                    .name
                    .as_ref()
                    .map(|n| format!(" \"{}\"", truncate(n)))
                    .unwrap_or_default();
                println!("    {}. {} {label}", i + 1, ancestor.role.short_name());
            }
        }

        Ok(())
    }
}

/// Screenshot and run OCR, returning recognized text with coordinates.
#[derive(clap::Args)]
#[command(about = "Screenshot and run OCR, returning recognized text with coordinates")]
pub struct Ocr {
    #[command(flatten)]
    pub global: GlobalOptions,

    #[arg(long, help = "Filter results containing this text")]
    pub find: Option<String>,

    #[arg(
        long,
        help = "Skip saving the display screenshot (only output OCR text)"
    )]
    pub no_screenshot: bool,

    #[arg(long, help = "Image format for screenshot: jpeg, png, or webp")]
    pub format: Option<String>,

    #[arg(long, help = "JPEG quality 1-100 (default 85)")]
    pub quality: Option<u32>,

    #[arg(long, help = "Output scale: 1 or 2 (default 1)")]
    pub scale: Option<u32>,

    #[arg(long, help = "Exclude mouse cursor from screenshot")]
    pub no_cursor: bool,
}

impl Ocr {
    /// Runs OCR on a screenshot and prints recognized text with coordinates.
    ///
    /// # Errors
    ///
    /// Returns an error if screen recording permission is denied,
    /// or the target app/window is not found.
    pub fn run(&self, provider: &dyn DesktopProvider) -> anyhow::Result<()> {
        let ss_options: Option<ScreenshotOptions> = if self.no_screenshot {
            None
        } else {
            Some(self.build_screenshot_options())
        };

        let app_target = self.global.app_target.resolve()?;
        let window_target = self.global.window_target.resolve();
        let output = provider.ocr(
            app_target.as_ref(),
            window_target.as_ref(),
            self.find.as_deref(),
            ss_options.as_ref(),
        )?;

        // Print screenshot path first
        if let Some(ref path) = output.screenshot_path {
            println!("{path}");
        }

        if self.global.json {
            for r in &output.results {
                let (cx, cy) = r.center();
                println!(
                    "{{\"text\": \"{}\", \"x\": {:.0}, \"y\": {:.0}, \"bounds\": {{\"x\": {:.0}, \"y\": {:.0}, \"w\": {:.0}, \"h\": {:.0}}}}}",
                    r.text,
                    cx, cy,
                    r.bounds.x, r.bounds.y,
                    r.bounds.width, r.bounds.height
                );
            }
        } else {
            for r in &output.results {
                let (cx, cy) = r.center();
                println!("{}  [{:.0},{:.0}]", r.text, cx, cy);
            }
        }

        if output.results.is_empty() {
            if let Some(ref find) = self.find {
                println!("No text matching '{find}' found");
            } else {
                println!("No text recognized");
            }
        }

        Ok(())
    }

    fn build_screenshot_options(&self) -> ScreenshotOptions {
        let fmt = self
            .format
            .as_deref()
            .and_then(|s| s.parse().ok())
            .unwrap_or(ImageFormat::BestAvailable);

        ScreenshotOptions {
            format: fmt,
            quality: self.quality.unwrap_or(85),
            scale: self.scale.unwrap_or(1),
            cursor: !self.no_cursor,
        }
    }
}
