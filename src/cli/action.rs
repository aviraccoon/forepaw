use crate::cli::parse::{parse_coordinate, parse_region, resolve_text, shell_split};
/// CLI subcommands: actions (click, type, press, scroll, drag, hover, wait, batch, ocr-click).
use crate::core::element_tree::ElementRef;
use crate::core::key_combo::{ClickOptions, DragOptions, KeyCombo, MouseButton};
use crate::core::output_formatter::OutputFormatter;
use crate::platform::DesktopProvider;

/// Click an element by ref or at coordinates.
#[derive(clap::Args)]
#[command(about = "Click an element by ref or at coordinates")]
pub struct Click {
    #[arg(help = "Element ref (@e3), coordinates (500,300), or region (400,280,80,80)")]
    pub target: String,

    #[arg(
        long,
        help = "Target application name (required; coordinates are relative to window)"
    )]
    pub app: Option<String>,

    #[arg(long, help = "Right-click (context menu)")]
    pub right: bool,

    #[arg(long, help = "Double-click")]
    pub double: bool,

    #[arg(long, help = "JSON output")]
    pub json: bool,
}

impl Click {
    pub fn run(&self, provider: &dyn DesktopProvider) -> anyhow::Result<()> {
        let options = ClickOptions::new(
            if self.right {
                MouseButton::Right
            } else {
                MouseButton::Left
            },
            if self.double { 2 } else { 1 },
        );

        let result = if let Some(element_ref) = ElementRef::parse(&self.target) {
            let app = self
                .app
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("--app is required for ref-based click"))?;
            provider.click_ref(element_ref, app, &options)?
        } else if let Some(region) = parse_region(&self.target) {
            let app = self
                .app
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("--app is required for region-based click"))?;
            provider.click_region(region, app, None, &options)?
        } else if let Some(point) = parse_coordinate(&self.target) {
            let app = self
                .app
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("--app is required for coordinate-based click"))?;
            provider.click_at_point(point, app, &options)?
        } else {
            return Err(anyhow::anyhow!(
                "Invalid target: {}. Expected a ref (@e1), coordinates (500,300), or region (400,280,80,80).",
                self.target
            ));
        };

        let formatter = OutputFormatter::new(self.json);
        print!(
            "{}",
            formatter.format(
                result.success,
                "click",
                &[("text", result.message.as_deref().unwrap_or("clicked"))],
                None,
            )
        );
        Ok(())
    }
}

/// Type text into an element.
#[derive(clap::Args)]
#[command(about = "Type text into an element")]
pub struct Type {
    #[arg(help = "Element ref (e.g. @e5)")]
    pub r#ref: String,

    #[arg(help = "Text to type")]
    pub positional_text: Option<String>,

    #[arg(
        long = "text",
        help = "Text to type (use instead of positional for text starting with dashes)"
    )]
    pub text_option: Option<String>,

    #[arg(long, help = "Target application name")]
    pub app: String,

    #[arg(long, help = "JSON output")]
    pub json: bool,
}

impl Type {
    pub fn run(&self, provider: &dyn DesktopProvider) -> anyhow::Result<()> {
        let text = resolve_text(
            self.positional_text.as_deref(),
            self.text_option.as_deref(),
            "type",
        )?;
        let element_ref = ElementRef::parse(&self.r#ref).ok_or_else(|| {
            anyhow::anyhow!(
                "Invalid ref: {}. Expected format: @e1, @e2, etc.",
                self.r#ref
            )
        })?;
        let result = provider.type_ref(element_ref, text, &self.app)?;

        let formatter = OutputFormatter::new(self.json);
        print!(
            "{}",
            formatter.format(
                result.success,
                "type",
                &[("text", result.message.as_deref().unwrap_or("typed"))],
                None,
            )
        );
        Ok(())
    }
}

/// Type text into the focused element (no ref needed).
#[derive(clap::Args)]
#[command(
    name = "keyboard-type",
    about = "Type text into the focused element (no ref needed)"
)]
pub struct KeyboardType {
    #[arg(help = "Text to type")]
    pub positional_text: Option<String>,

    #[arg(
        long = "text",
        help = "Text to type (use instead of positional for text starting with dashes)"
    )]
    pub text_option: Option<String>,

    #[arg(
        long,
        help = "Target application name (activates app first; omit to type into current focus)"
    )]
    pub app: Option<String>,

    #[arg(long, help = "JSON output")]
    pub json: bool,
}

impl KeyboardType {
    pub fn run(&self, provider: &dyn DesktopProvider) -> anyhow::Result<()> {
        let text = resolve_text(
            self.positional_text.as_deref(),
            self.text_option.as_deref(),
            "keyboard-type",
        )?;
        let result = provider.keyboard_type(text, self.app.as_deref())?;

        let formatter = OutputFormatter::new(self.json);
        print!(
            "{}",
            formatter.format(
                result.success,
                "keyboard-type",
                &[("text", result.message.as_deref().unwrap_or("typed"))],
                None,
            )
        );
        Ok(())
    }
}

/// Press a keyboard shortcut.
#[derive(clap::Args)]
#[command(about = "Press a keyboard shortcut (e.g. cmd+s, ctrl+shift+z)")]
pub struct Press {
    #[arg(help = "Key combo (e.g. cmd+s, return, escape)")]
    pub combo: String,

    #[arg(
        long,
        help = "Target application name (activates app first; omit for global hotkeys)"
    )]
    pub app: Option<String>,

    #[arg(long, help = "JSON output")]
    pub json: bool,
}

impl Press {
    pub fn run(&self, provider: &dyn DesktopProvider) -> anyhow::Result<()> {
        let key_combo = KeyCombo::parse(&self.combo);
        let result = provider.press(&key_combo, self.app.as_deref())?;

        let formatter = OutputFormatter::new(self.json);
        print!(
            "{}",
            formatter.format(
                result.success,
                "press",
                &[("text", result.message.as_deref().unwrap_or("pressed"))],
                None,
            )
        );
        Ok(())
    }
}

/// Find text on screen via OCR and click it.
#[derive(clap::Args)]
#[command(name = "ocr-click", about = "Find text on screen via OCR and click it")]
pub struct OcrClick {
    #[arg(help = "Text to find and click")]
    pub positional_text: Option<String>,

    #[arg(
        long = "text",
        help = "Text to find and click (use instead of positional for text starting with dashes)"
    )]
    pub text_option: Option<String>,

    #[arg(long, help = "Target application name")]
    pub app: String,

    #[arg(long, help = "Window title or ID")]
    pub window: Option<String>,

    #[arg(long, help = "Right-click (context menu)")]
    pub right: bool,

    #[arg(long, help = "Double-click")]
    pub double: bool,

    #[arg(long, help = "Which match to click (1-based) when multiple found")]
    pub index: Option<usize>,

    #[arg(long, help = "JSON output")]
    pub json: bool,
}

impl OcrClick {
    pub fn run(&self, provider: &dyn DesktopProvider) -> anyhow::Result<()> {
        let text = resolve_text(
            self.positional_text.as_deref(),
            self.text_option.as_deref(),
            "ocr-click",
        )?;
        let options = ClickOptions::new(
            if self.right {
                MouseButton::Right
            } else {
                MouseButton::Left
            },
            if self.double { 2 } else { 1 },
        );
        let result = provider.ocr_click(
            text,
            &self.app,
            self.window.as_deref(),
            &options,
            self.index,
        )?;

        let formatter = OutputFormatter::new(self.json);
        print!(
            "{}",
            formatter.format(
                result.success,
                "ocr-click",
                &[("text", result.message.as_deref().unwrap_or("clicked"))],
                None,
            )
        );
        Ok(())
    }
}

/// Hover over an element or at coordinates.
#[derive(clap::Args)]
#[command(about = "Move mouse to an element without clicking (triggers tooltips/hover states)")]
pub struct Hover {
    #[arg(
        help = "Element ref (@e3), text for OCR, coordinates (500,300), or region (400,280,80,80)"
    )]
    pub target: String,

    #[arg(long, help = "Target application name")]
    pub app: Option<String>,

    #[arg(long, help = "Window title or ID")]
    pub window: Option<String>,

    #[arg(long, help = "Smooth mouse movement")]
    pub smooth: bool,

    #[arg(long, help = "JSON output")]
    pub json: bool,
}

impl Hover {
    pub fn run(&self, provider: &dyn DesktopProvider) -> anyhow::Result<()> {
        let result = if let Some(element_ref) = ElementRef::parse(&self.target) {
            let app = self
                .app
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("--app is required for ref-based hover"))?;
            provider.hover_ref(element_ref, app)?
        } else if let Some(region) = parse_region(&self.target) {
            let app = self
                .app
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("--app is required for region-based hover"))?;
            provider.hover_region(region, app, self.window.as_deref(), self.smooth)?
        } else if let Some(point) = parse_coordinate(&self.target) {
            provider.hover_at_point(point, self.app.as_deref(), self.smooth)?
        } else {
            let app = self
                .app
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("--app is required for text-based hover"))?;
            provider.ocr_hover(&self.target, app, self.window.as_deref(), None)?
        };

        let formatter = OutputFormatter::new(self.json);
        print!(
            "{}",
            formatter.format(
                result.success,
                "hover",
                &[("text", result.message.as_deref().unwrap_or("hovered"))],
                None,
            )
        );
        Ok(())
    }
}

/// Wait for text to appear on screen (OCR polling).
#[derive(clap::Args)]
#[command(about = "Wait for text to appear on screen (OCR polling)")]
pub struct Wait {
    #[arg(help = "Text to wait for")]
    pub positional_text: Option<String>,

    #[arg(
        long = "text",
        help = "Text to wait for (use instead of positional for text starting with dashes)"
    )]
    pub text_option: Option<String>,

    #[arg(long, help = "Target application name")]
    pub app: String,

    #[arg(long, help = "Window title or ID")]
    pub window: Option<String>,

    #[arg(long, help = "Maximum seconds to wait (default 10)")]
    pub timeout: Option<f64>,

    #[arg(long, help = "Seconds between polls (default 1)")]
    pub interval: Option<f64>,

    #[arg(long, help = "JSON output")]
    pub json: bool,
}

impl Wait {
    pub fn run(&self, provider: &dyn DesktopProvider) -> anyhow::Result<()> {
        let text = resolve_text(
            self.positional_text.as_deref(),
            self.text_option.as_deref(),
            "wait",
        )?;
        let result = provider.wait(
            text,
            &self.app,
            self.window.as_deref(),
            self.timeout.unwrap_or(10.0),
            self.interval.unwrap_or(1.0),
        )?;

        let formatter = OutputFormatter::new(self.json);
        print!(
            "{}",
            formatter.format(
                result.success,
                "wait",
                &[("text", result.message.as_deref().unwrap_or("found"))],
                None,
            )
        );
        Ok(())
    }
}

/// Scroll within an app window.
#[derive(clap::Args)]
#[command(about = "Scroll within an app window")]
pub struct Scroll {
    #[arg(help = "Direction: up, down, left, right")]
    pub direction: String,

    #[arg(long, help = "Target application name")]
    pub app: String,

    #[arg(long, help = "Window title or ID")]
    pub window: Option<String>,

    #[arg(short, long, help = "Number of scroll ticks (default 3)")]
    pub amount: Option<u32>,

    #[arg(long, help = "Element ref to scroll within")]
    pub r#ref: Option<String>,

    #[arg(long, help = "Window-relative coordinates to scroll at")]
    pub at: Option<String>,

    #[arg(long, help = "JSON output")]
    pub json: bool,
}

impl Scroll {
    pub fn run(&self, provider: &dyn DesktopProvider) -> anyhow::Result<()> {
        let valid = ["up", "down", "left", "right"];
        if !valid.contains(&self.direction.as_str()) {
            anyhow::bail!(
                "Invalid direction '{}'. Use: {}",
                self.direction,
                valid.join(", ")
            );
        }

        if self.r#ref.is_some() && self.at.is_some() {
            anyhow::bail!("Use --ref or --at, not both");
        }

        let element_ref = self
            .r#ref
            .as_deref()
            .map(|s| {
                ElementRef::parse(s)
                    .ok_or_else(|| anyhow::anyhow!("Invalid ref: {s}. Expected format: @e1"))
            })
            .transpose()?;

        let scroll_point = self
            .at
            .as_deref()
            .map(|s| {
                parse_coordinate(s)
                    .ok_or_else(|| anyhow::anyhow!("Invalid coordinates: {s}. Expected x,y"))
            })
            .transpose()?;

        let result = provider.scroll(
            &self.direction,
            self.amount.unwrap_or(3),
            &self.app,
            self.window.as_deref(),
            element_ref,
            scroll_point,
        )?;

        let formatter = OutputFormatter::new(self.json);
        print!(
            "{}",
            formatter.format(
                result.success,
                "scroll",
                &[("text", result.message.as_deref().unwrap_or("scrolled"))],
                None,
            )
        );
        Ok(())
    }
}

/// Drag from one point to another.
#[derive(clap::Args)]
#[command(about = "Drag from one point to another (for drawing, moving, resizing)")]
pub struct Drag {
    #[arg(
        trailing_var_arg = true,
        help = "Drag targets: <from> <to> or path (coords as x,y or refs as @eN)"
    )]
    pub targets: Vec<String>,

    #[arg(long, help = "Target application name")]
    pub app: Option<String>,

    #[arg(long, help = "Number of intermediate steps per segment (default 30)")]
    pub steps: Option<u32>,

    #[arg(long, help = "Total duration in seconds (default 0.3)")]
    pub duration: Option<f64>,

    #[arg(long, help = "Hold modifier keys during drag (e.g. shift, shift+alt)")]
    pub modifiers: Option<String>,

    #[arg(long, help = "Mouse pressure 0.0-1.0")]
    pub pressure: Option<f64>,

    #[arg(long, help = "Use right mouse button")]
    pub right: bool,

    #[arg(long, help = "Close path by appending start point")]
    pub close: bool,

    #[arg(long, help = "Read coordinates from stdin")]
    pub stdin: bool,

    #[arg(long, help = "JSON output")]
    pub json: bool,
}

impl Drag {
    pub fn run(&self, provider: &dyn DesktopProvider) -> anyhow::Result<()> {
        let options = DragOptions {
            steps: self.steps.unwrap_or(30),
            duration: self.duration.unwrap_or(0.3),
            modifiers: crate::core::key_combo::Modifier::parse_modifiers(self.modifiers.as_deref()),
            pressure: self.pressure,
            right_button: self.right,
            close_path: self.close,
        };

        let result = if self.stdin {
            let coords = read_coords_from_stdin()?;
            if coords.len() < 2 {
                anyhow::bail!(
                    "--stdin requires at least 2 coordinates. Got {}.",
                    coords.len()
                );
            }
            provider.drag_path(&coords, &options, self.app.as_deref())?
        } else if self.targets.len() < 2 {
            anyhow::bail!(
                "drag requires at least 2 targets.\n\
                 Examples: drag 100,100 500,500 --app Finder\n\
                 \t      drag @e3 @e7 --app Finder"
            );
        } else {
            let coords: Vec<_> = self
                .targets
                .iter()
                .filter_map(|t| parse_coordinate(t))
                .collect();
            if coords.len() == self.targets.len() {
                provider.drag_path(&coords, &options, self.app.as_deref())?
            } else if self.targets.len() == 2 {
                let from_ref = ElementRef::parse(&self.targets[0]);
                let to_ref = ElementRef::parse(&self.targets[1]);
                if let (Some(from), Some(to)) = (from_ref, to_ref) {
                    let app = self
                        .app
                        .as_deref()
                        .ok_or_else(|| anyhow::anyhow!("--app is required for ref-based drag"))?;
                    provider.drag_refs(from, to, app, &options)?
                } else {
                    // Mixed targets: resolve each
                    let from = resolve_drag_target(&self.targets[0], provider)?;
                    let to = resolve_drag_target(&self.targets[1], provider)?;
                    provider.drag_path(&[from, to], &options, self.app.as_deref())?
                }
            } else {
                return Err(anyhow::anyhow!(
                    "Path mode requires all coordinates. Refs only supported for 2-point drag."
                ));
            }
        };

        let formatter = OutputFormatter::new(self.json);
        print!(
            "{}",
            formatter.format(
                result.success,
                "drag",
                &[("text", result.message.as_deref().unwrap_or("dragged"))],
                None,
            )
        );
        Ok(())
    }
}

fn resolve_drag_target(
    target: &str,
    _provider: &dyn DesktopProvider,
) -> anyhow::Result<crate::core::types::Point> {
    if let Some(point) = parse_coordinate(target) {
        return Ok(point);
    }
    // Ref-based resolution needs the provider, but we'd need app context
    // For now, just fail
    anyhow::bail!("Invalid target: {target}. Expected coordinates (500,300).")
}

fn read_coords_from_stdin() -> anyhow::Result<Vec<crate::core::types::Point>> {
    use std::io::Read;
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;

    let tokens: Vec<&str> = input
        .split(|c: char| c.is_whitespace())
        .filter(|t| !t.is_empty())
        .collect();

    let coords: Vec<_> = tokens.iter().filter_map(|t| parse_coordinate(t)).collect();
    if coords.len() != tokens.len() {
        let bad: Vec<_> = tokens
            .iter()
            .filter(|t| parse_coordinate(t).is_none())
            .collect();
        anyhow::bail!(
            "Invalid coordinate(s): {}",
            bad.iter().map(|s| **s).collect::<Vec<_>>().join(", ")
        );
    }
    Ok(coords)
}

/// Execute multiple actions in one invocation.
#[derive(clap::Args)]
#[command(about = "Execute multiple actions in one invocation")]
pub struct Batch {
    #[arg(
        trailing_var_arg = true,
        help = "Actions separated by ;; (e.g. 'click @e3 ;; press cmd+s ;; keyboard-type hello')"
    )]
    pub args: Vec<String>,

    #[arg(long, help = "Target application name")]
    pub app: Option<String>,

    #[arg(long, help = "Window title or ID")]
    pub window: Option<String>,

    #[arg(long, help = "Delay in milliseconds between actions (default 100)")]
    pub delay: Option<u64>,

    #[arg(long, help = "JSON output")]
    pub json: bool,
}

impl Batch {
    pub fn run(&self, provider: &dyn DesktopProvider) -> anyhow::Result<()> {
        let joined = self.args.join(" ");
        let actions: Vec<&str> = joined
            .split(";;")
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();

        if actions.is_empty() {
            anyhow::bail!("No actions provided. Separate actions with ;;");
        }

        let delay_ms = self.delay.unwrap_or(100);
        let formatter = OutputFormatter::new(self.json);

        for (i, action) in actions.iter().enumerate() {
            let result = execute_action(
                action,
                provider,
                self.app.as_deref(),
                self.window.as_deref(),
            )?;
            print!(
                "{}",
                formatter.format(
                    result.success,
                    action,
                    &[("text", result.message.as_deref().unwrap_or("ok"))],
                    None,
                )
            );

            if i < actions.len() - 1 {
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
            }
        }

        Ok(())
    }
}

fn execute_action(
    action: &str,
    provider: &dyn DesktopProvider,
    batch_app: Option<&str>,
    batch_window: Option<&str>,
) -> anyhow::Result<crate::platform::ActionResult> {
    let parts = shell_split(action);
    let command = parts
        .first()
        .ok_or_else(|| anyhow::anyhow!("Empty action"))?;
    let args = &parts[1..];

    match command.as_str() {
        "click" => {
            let target = args
                .first()
                .ok_or_else(|| anyhow::anyhow!("click requires a target"))?;
            let app_name = parse_option("--app", args).or(batch_app);
            let options = ClickOptions::new(
                if args.iter().any(|a| a == "--right") {
                    MouseButton::Right
                } else {
                    MouseButton::Left
                },
                if args.iter().any(|a| a == "--double") {
                    2
                } else {
                    1
                },
            );

            if let Some(r#ref) = ElementRef::parse(target) {
                let app = app_name.ok_or_else(|| anyhow::anyhow!("click requires --app"))?;
                provider.click_ref(r#ref, app, &options).map_err(Into::into)
            } else if let Some(region) = parse_region(target) {
                let app = app_name.ok_or_else(|| anyhow::anyhow!("click requires --app"))?;
                let win = parse_option("--window", args)
                    .map(String::from)
                    .or_else(|| batch_window.map(String::from));
                provider
                    .click_region(region, app, win.as_deref(), &options)
                    .map_err(Into::into)
            } else if let Some(point) = parse_coordinate(target) {
                let app = app_name.ok_or_else(|| anyhow::anyhow!("click requires --app"))?;
                provider
                    .click_at_point(point, app, &options)
                    .map_err(Into::into)
            } else {
                anyhow::bail!(
                    "Invalid click target: {target}. Use ref (@e3), coords (500,300), or region (400,280,80,80)"
                );
            }
        }
        "hover" => {
            let target = args
                .first()
                .ok_or_else(|| anyhow::anyhow!("hover requires a target"))?;
            let app_name = parse_option("--app", args).or(batch_app);
            let smooth = args.iter().any(|a| a == "--smooth");

            if let Some(r#ref) = ElementRef::parse(target) {
                let app = app_name.ok_or_else(|| anyhow::anyhow!("hover requires --app"))?;
                provider.hover_ref(r#ref, app).map_err(Into::into)
            } else if let Some(region) = parse_region(target) {
                let app = app_name.ok_or_else(|| anyhow::anyhow!("hover requires --app"))?;
                let win = parse_option("--window", args)
                    .map(String::from)
                    .or_else(|| batch_window.map(String::from));
                provider
                    .hover_region(region, app, win.as_deref(), smooth)
                    .map_err(Into::into)
            } else if let Some(point) = parse_coordinate(target) {
                provider
                    .hover_at_point(point, app_name, smooth)
                    .map_err(Into::into)
            } else {
                let app = app_name.ok_or_else(|| anyhow::anyhow!("hover requires --app"))?;
                let win = parse_option("--window", args)
                    .map(String::from)
                    .or_else(|| batch_window.map(String::from));
                provider
                    .ocr_hover(target, app, win.as_deref(), None)
                    .map_err(Into::into)
            }
        }
        "type" => {
            let ref_str = args
                .first()
                .ok_or_else(|| anyhow::anyhow!("type requires ref and text"))?;
            let r#ref = ElementRef::parse(ref_str)
                .ok_or_else(|| anyhow::anyhow!("Invalid ref: {ref_str}"))?;
            let text = parse_option("--text", args)
                .map(String::from)
                .or_else(|| collect_positional_text(args, 1));
            let text = text.ok_or_else(|| anyhow::anyhow!("type requires text"))?;
            let app_name = parse_option("--app", args)
                .map(String::from)
                .or_else(|| batch_app.map(String::from))
                .ok_or_else(|| anyhow::anyhow!("type requires --app"))?;
            provider
                .type_ref(r#ref, &text, &app_name)
                .map_err(Into::into)
        }
        "keyboard-type" => {
            let text = parse_option("--text", args)
                .map(String::from)
                .or_else(|| collect_positional_text(args, 0));
            let text = text.ok_or_else(|| anyhow::anyhow!("keyboard-type requires text"))?;
            let app_name = parse_option("--app", args)
                .map(String::from)
                .or_else(|| batch_app.map(String::from));
            provider
                .keyboard_type(&text, app_name.as_deref())
                .map_err(Into::into)
        }
        "press" => {
            let combo_str = args
                .first()
                .ok_or_else(|| anyhow::anyhow!("press requires a key combo"))?;
            let key_combo = KeyCombo::parse(combo_str);
            let app_name = parse_option("--app", args)
                .map(String::from)
                .or_else(|| batch_app.map(String::from));
            provider
                .press(&key_combo, app_name.as_deref())
                .map_err(Into::into)
        }
        "scroll" => {
            let direction = args
                .first()
                .ok_or_else(|| anyhow::anyhow!("scroll requires a direction"))?;
            let app_name = parse_option("--app", args)
                .map(String::from)
                .or_else(|| batch_app.map(String::from))
                .ok_or_else(|| anyhow::anyhow!("scroll requires --app"))?;
            let amount = parse_option("--amount", args)
                .and_then(|s| s.parse().ok())
                .unwrap_or(3);
            let win = parse_option("--window", args)
                .map(String::from)
                .or_else(|| batch_window.map(String::from));
            let scroll_ref = parse_option("--ref", args).and_then(ElementRef::parse);
            let scroll_at = parse_option("--at", args).and_then(parse_coordinate);
            provider
                .scroll(
                    direction,
                    amount,
                    &app_name,
                    win.as_deref(),
                    scroll_ref,
                    scroll_at,
                )
                .map_err(Into::into)
        }
        "ocr-click" => {
            let text = parse_option("--text", args)
                .map(String::from)
                .or_else(|| args.first().map(String::from));
            let text = text.ok_or_else(|| anyhow::anyhow!("ocr-click requires text"))?;
            let app_name = parse_option("--app", args)
                .map(String::from)
                .or_else(|| batch_app.map(String::from))
                .ok_or_else(|| anyhow::anyhow!("ocr-click requires --app"))?;
            let win = parse_option("--window", args)
                .map(String::from)
                .or_else(|| batch_window.map(String::from));
            let options = ClickOptions::new(
                if args.iter().any(|a| a == "--right") {
                    MouseButton::Right
                } else {
                    MouseButton::Left
                },
                if args.iter().any(|a| a == "--double") {
                    2
                } else {
                    1
                },
            );
            let ocr_index = parse_option("--index", args).and_then(|s| s.parse().ok());
            provider
                .ocr_click(&text, &app_name, win.as_deref(), &options, ocr_index)
                .map_err(Into::into)
        }
        "drag" => {
            if args.len() < 2 {
                anyhow::bail!("drag requires at least 2 targets");
            }
            let app_name = parse_option("--app", args)
                .map(String::from)
                .or_else(|| batch_app.map(String::from));
            let drag_steps = parse_option("--steps", args)
                .and_then(|s| s.parse().ok())
                .unwrap_or(30);
            let drag_duration = parse_option("--duration", args)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.3);
            let drag_pressure = parse_option("--pressure", args).and_then(|s| s.parse().ok());
            let drag_mods = crate::core::key_combo::Modifier::parse_modifiers(parse_option(
                "--modifiers",
                args,
            ));
            let drag_right = args.iter().any(|a| a == "--right");
            let drag_close = args.iter().any(|a| a == "--close");

            let drag_options = DragOptions {
                steps: drag_steps,
                duration: drag_duration,
                modifiers: drag_mods,
                pressure: drag_pressure,
                right_button: drag_right,
                close_path: drag_close,
            };

            let known_flags = ["--right", "--close"];
            let drag_targets: Vec<&str> = args
                .iter()
                .filter(|a| {
                    !a.starts_with("--")
                        && !known_flags.contains(&a.as_str())
                        && parse_option(a, args).is_none()
                })
                .take_while(|a| parse_coordinate(a).is_some() || ElementRef::parse(a).is_some())
                .map(String::as_str)
                .collect();

            let coords: Vec<_> = drag_targets
                .iter()
                .filter_map(|t| parse_coordinate(t))
                .collect();

            if coords.len() == drag_targets.len() && coords.len() >= 2 {
                provider
                    .drag_path(&coords, &drag_options, app_name.as_deref())
                    .map_err(Into::into)
            } else if drag_targets.len() == 2 {
                let from_ref = ElementRef::parse(drag_targets[0]);
                let to_ref = ElementRef::parse(drag_targets[1]);
                if let (Some(from), Some(to)) = (from_ref, to_ref) {
                    let app = app_name.ok_or_else(|| anyhow::anyhow!("drag requires --app"))?;
                    provider
                        .drag_refs(from, to, &app, &drag_options)
                        .map_err(Into::into)
                } else {
                    anyhow::bail!("Invalid drag targets. Use coordinates or refs")
                }
            } else {
                anyhow::bail!("Invalid drag targets")
            }
        }
        "wait" => {
            let text = parse_option("--text", args)
                .map(String::from)
                .or_else(|| args.first().map(String::from));
            let text = text.ok_or_else(|| anyhow::anyhow!("wait requires text"))?;
            let app_name = parse_option("--app", args)
                .map(String::from)
                .or_else(|| batch_app.map(String::from))
                .ok_or_else(|| anyhow::anyhow!("wait requires --app"))?;
            let win = parse_option("--window", args)
                .map(String::from)
                .or_else(|| batch_window.map(String::from));
            let timeout = parse_option("--timeout", args)
                .and_then(|s| s.parse().ok())
                .unwrap_or(10.0);
            let interval = parse_option("--interval", args)
                .and_then(|s| s.parse().ok())
                .unwrap_or(1.0);
            provider
                .wait(&text, &app_name, win.as_deref(), timeout, interval)
                .map_err(Into::into)
        }
        _ => {
            anyhow::bail!(
                "Unknown action '{command}'. Supported: click, hover, drag, type, keyboard-type, press, scroll, ocr-click, wait"
            );
        }
    }
}

/// Parse a named option value from an argument list.
fn parse_option<'a>(name: &str, args: &'a [String]) -> Option<&'a str> {
    let idx = args.iter().position(|a| a == name)?;
    args.get(idx + 1).map(String::as_str)
}

/// Collect remaining positional arguments as text, skipping flags and their values.
fn collect_positional_text(args: &[String], skip: usize) -> Option<String> {
    let mut positional: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i].starts_with("--") {
            i += 2; // skip flag + value
            continue;
        }
        positional.push(&args[i]);
        i += 1;
    }
    let remaining: Vec<&str> = positional.into_iter().skip(skip).collect();
    if remaining.is_empty() {
        None
    } else {
        Some(remaining.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_option ---

    #[test]
    fn parse_option_found() {
        let args: Vec<String> = ["click", "@e3", "--app", "Finder"]
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        assert_eq!(parse_option("--app", &args), Some("Finder"));
    }

    #[test]
    fn parse_option_missing() {
        let args: Vec<String> = ["click", "@e3"]
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        assert_eq!(parse_option("--app", &args), None);
    }

    #[test]
    fn parse_option_last_arg_no_value() {
        let args: Vec<String> = ["click", "--app"]
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        assert_eq!(parse_option("--app", &args), None);
    }

    #[test]
    fn parse_option_first_match() {
        let args: Vec<String> = ["--amount", "5", "--amount", "10"]
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        assert_eq!(parse_option("--amount", &args), Some("5"));
    }

    // --- collect_positional_text ---

    #[test]
    fn collect_positional_simple() {
        let args: Vec<String> = ["@e3", "hello", "world"]
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        assert_eq!(
            collect_positional_text(&args, 1),
            Some("hello world".to_string())
        );
    }

    #[test]
    fn collect_positional_skips_flags() {
        let args: Vec<String> = ["@e3", "--text", "ignored", "actual", "text"]
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        assert_eq!(
            collect_positional_text(&args, 1),
            Some("actual text".to_string())
        );
    }

    #[test]
    fn collect_positional_no_positional_after_skip() {
        let args: Vec<String> = ["@e3"]
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        assert_eq!(collect_positional_text(&args, 1), None);
    }

    #[test]
    fn collect_positional_empty_args() {
        assert_eq!(collect_positional_text(&[], 0), None);
    }

    #[test]
    fn collect_positional_zero_skip() {
        let args: Vec<String> = ["hello", "world"]
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        assert_eq!(
            collect_positional_text(&args, 0),
            Some("hello world".to_string())
        );
    }

    #[test]
    fn collect_positional_only_flags() {
        let args: Vec<String> = ["--text", "val", "--app", "Finder"]
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        assert_eq!(collect_positional_text(&args, 0), None);
    }
}
