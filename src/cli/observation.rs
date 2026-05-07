/// CLI subcommands: observation (snapshot, screenshot, list-apps, list-windows, ocr).
use clap::Args;

use crate::cli::parse::parse_region;
use crate::core::annotation::AnnotationStyle;
use crate::core::crop_region::CropRegion;
use crate::core::element_tree::ElementRef;
use crate::core::snapshot_cache::SnapshotCache;
use crate::core::snapshot_diff::SnapshotDiffer;
use crate::core::tree_renderer::TreeRenderer;
use crate::platform::{DesktopProvider, ImageFormat, ScreenshotOptions, SnapshotOptions};

/// Shared global options (app, window, json).
#[derive(Args, Clone)]
pub struct GlobalOptions {
    #[arg(long, help = "Target application name")]
    pub app: Option<String>,

    #[arg(long, help = "Window title or ID (e.g. 'Hacker News' or 'w-7290')")]
    pub window: Option<String>,

    #[arg(long, help = "JSON output")]
    pub json: bool,
}

/// Accessibility tree with element refs.
#[derive(clap::Args)]
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
    pub fn run(&self, provider: &dyn DesktopProvider) -> anyhow::Result<()> {
        let app = self
            .global
            .app
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("--app is required"))?;

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

        let tree = provider.snapshot(app, &options)?;
        let renderer = TreeRenderer::new();
        let rendered = renderer.render(&tree);

        let cache = SnapshotCache::new();

        if self.diff {
            if let Some(previous) = cache.load(app) {
                let differ = SnapshotDiffer::new();
                let result = differ.diff(&previous, &rendered);
                println!("{}", result.render(self.context));
            } else {
                println!("[diff: no previous snapshot cached for {}]", app);
                println!("{}", rendered);
            }
        } else {
            println!("{}", rendered);
        }

        if let Some(ref timing_data) = tree.timing {
            let report = timing_data.report();
            eprintln!("{}", report);
        }

        // Always cache for future diffs
        let _ = cache.save(app, &rendered);

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

        let params = crate::platform::ScreenshotParams {
            app: self.global.app.as_deref(),
            window: self.global.window.as_deref(),
            style: annotation_style,
            only: ref_filter.as_deref(),
            options: &ss_options,
            crop: crop_region.as_ref(),
            grid_spacing: self.grid,
        };
        let result = provider.screenshot(&params)?;

        println!("{}", result.path);
        if let Some(legend) = &result.legend {
            println!("{}", legend);
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
                .app
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("--ref requires --app"))?;
            let element_ref = ElementRef::parse(ref_str)
                .ok_or_else(|| anyhow::anyhow!("Invalid ref format: {}. Expected @eN", ref_str))?;
            let bounds = provider.resolve_ref_bounds(element_ref, app)?;
            return Ok(Some(CropRegion::new(bounds, pad)));
        }

        if let Some(ref region) = self.region {
            let rect = parse_region(region).ok_or_else(|| {
                anyhow::anyhow!("Invalid region format: {}. Expected x,y,w,h", region)
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
                        .map(|b| format!(", \"bundleID\": \"{}\"", b))
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
                    .map(|b| format!(" ({})", b))
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
    pub fn run(&self, provider: &dyn DesktopProvider) -> anyhow::Result<()> {
        let windows = provider.list_windows(self.global.app.as_deref())?;
        for w in windows {
            println!("{}  {}  \"{}\"", w.id, w.app, w.title);
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
    pub fn run(&self, provider: &dyn DesktopProvider) -> anyhow::Result<()> {
        let ss_options: Option<ScreenshotOptions> = if self.no_screenshot {
            None
        } else {
            Some(self.build_screenshot_options())
        };

        let output = provider.ocr(
            self.global.app.as_deref(),
            self.global.window.as_deref(),
            self.find.as_deref(),
            ss_options.as_ref(),
        )?;

        // Print screenshot path first
        if let Some(ref path) = output.screenshot_path {
            println!("{}", path);
        }

        if self.global.json {
            for r in &output.results {
                let (cx, cy) = r.center();
                println!(
                    "{{\"text\": \"{}\", \"x\": {}, \"y\": {}, \"bounds\": {{\"x\": {}, \"y\": {}, \"w\": {}, \"h\": {}}}}}",
                    r.text,
                    cx as i64, cy as i64,
                    r.bounds.x as i64, r.bounds.y as i64,
                    r.bounds.width as i64, r.bounds.height as i64
                );
            }
        } else {
            for r in &output.results {
                let (cx, cy) = r.center();
                println!("{}  [{},{}]", r.text, cx as i64, cy as i64);
            }
        }

        if output.results.is_empty() {
            if let Some(ref find) = self.find {
                println!("No text matching '{}' found", find);
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
