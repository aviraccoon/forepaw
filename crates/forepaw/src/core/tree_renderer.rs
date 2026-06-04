//! Renders an `ElementNode` tree as indented text.
use crate::core::element_tree::ElementTree;
use crate::core::types::Rect;

/// Renders an `ElementNode` tree as indented text.
#[derive(Debug)]
pub struct TreeRenderer {
    /// Whether to show verbose output (native role, identifier, attributes).
    verbose: bool,
}

impl TreeRenderer {
    /// Create a new renderer.
    #[must_use]
    pub fn new(verbose: bool) -> Self {
        Self { verbose }
    }

    /// Render the tree as a formatted string.
    #[must_use]
    pub fn render(&self, tree: &ElementTree) -> String {
        let mut lines: Vec<String> = Vec::new();

        // Header: app name and window bounds
        if let Some(wb) = &tree.window_bounds {
            lines.push(format!(
                "app: {}  window: [{:.0},{:.0} {:.0}x{:.0}]",
                tree.app, wb.x, wb.y, wb.width, wb.height
            ));
        } else {
            lines.push(format!("app: {}", tree.app));
        }

        Self::render_node(
            &tree.root,
            0,
            tree.window_bounds.as_ref(),
            self.verbose,
            &mut lines,
        );
        lines.join("\n")
    }

    fn render_node(
        node: &crate::core::element_tree::ElementNode,
        indent: usize,
        window_origin: Option<&Rect>,
        verbose: bool,
        lines: &mut Vec<String>,
    ) {
        let prefix = "  ".repeat(indent);
        let mut parts: Vec<String> = Vec::new();

        // Role (lowercase for prose)
        let role = node.data.role.to_lowercase();
        parts.push(role);

        // Ref
        if let Some(r) = &node.data.reference {
            parts.push(r.to_string());
        }

        // Name
        if let Some(name) = &node.data.name {
            if !name.is_empty() {
                parts.push(format!("\"{name}\""));
            }
        }

        // Value (truncated for display)
        if let Some(value) = &node.data.value {
            if !value.is_empty() {
                let display = if value.len() > 80 {
                    let truncated: String = value.chars().take(77).collect();
                    format!("{truncated}...")
                } else {
                    value.clone()
                };
                parts.push(format!("value=\"{display}\""));
            }
        }

        // Element state
        let mut state_parts: Vec<&'static str> = Vec::new();
        if let Some(false) = node.data.enabled {
            state_parts.push("disabled");
        }
        if node.data.focused == Some(true) {
            state_parts.push("focused");
        }
        if node.data.selected == Some(true) {
            state_parts.push("selected");
        }
        if !state_parts.is_empty() {
            parts.push(state_parts.join(" "));
        }

        // Bounds (window-relative when window bounds are available)
        if let Some(b) = &node.data.bounds {
            let (display_x, display_y) = if let Some(w) = window_origin {
                ((b.x - w.x).round(), (b.y - w.y).round())
            } else {
                (b.x.round(), b.y.round())
            };
            parts.push(format!(
                "({display_x:.0},{display_y:.0} {:.0}x{:.0})",
                b.width.round(),
                b.height.round()
            ));
        }

        // Verbose: extra detail not shown by default
        if verbose {
            if let Some(desc) = &node.data.description {
                if !desc.is_empty() {
                    parts.push(format!("desc=\"{desc}\""));
                }
            }
            if let Some(nr) = &node.data.native_role {
                parts.push(format!("native_role={nr}"));
            }
            if let Some(id) = &node.data.identifier {
                parts.push(format!("id=\"{id}\""));
            }
            if let Some(uid) = node.data.uid {
                parts.push(format!("uid={uid}"));
            }
            if let Some(sig) = node.data.signature {
                parts.push(format!("sig=0x{sig:016x}"));
            }
            if let Some(sig) = node.data.signature_bounds {
                parts.push(format!("sig_b=0x{sig:016x}"));
            }
        }

        // Extra attributes (sorted by key)
        let mut sorted_attrs = node.data.attributes.clone();
        sorted_attrs.sort_by(|a, b| a.0.cmp(&b.0));
        for (key, val) in &sorted_attrs {
            parts.push(format!("{key}={val}"));
        }

        lines.push(format!("{prefix}{}", parts.join(" ")));

        for child in &node.children {
            Self::render_node(child, indent + 1, window_origin, verbose, lines);
        }
    }
}

impl Default for TreeRenderer {
    fn default() -> Self {
        Self::new(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::element_tree::{ElementData, ElementNode, ElementRef, ElementTree};
    use crate::core::types::Rect;

    use crate::core::role::Role;

    #[test]
    fn simple_tree() {
        let tree = ElementTree::new(
            "TestApp",
            ElementNode::new(ElementData::new(Role::Window).with_name("Main Window"))
                .with_children(vec![
                    ElementNode::new(
                        ElementData::new(Role::Button)
                            .with_name("OK")
                            .with_reference(ElementRef::new(1)),
                    ),
                    ElementNode::new(
                        ElementData::new(Role::TextField)
                            .with_name("Name")
                            .with_value("hello")
                            .with_reference(ElementRef::new(2)),
                    ),
                ]),
        );

        let renderer = TreeRenderer::new(false);
        let output = renderer.render(&tree);
        let lines: Vec<&str> = output.lines().collect();

        assert_eq!(lines[0], "app: TestApp");
        assert_eq!(lines[1], "window \"Main Window\"");
        assert_eq!(lines[2], "  button @e1 \"OK\"");
        assert_eq!(lines[3], "  textfield @e2 \"Name\" value=\"hello\"");
    }

    #[test]
    fn display_is_lowercase() {
        let tree = ElementTree::new("App", ElementNode::new(ElementData::new(Role::SplitGroup)));

        let renderer = TreeRenderer::new(false);
        let output = renderer.render(&tree);

        assert!(output.contains("splitgroup"));
        assert!(!output.contains("SplitGroup"));
    }

    #[test]
    fn truncates_long_values() {
        let long_value = "x".repeat(100);
        let tree = ElementTree::new(
            "App",
            ElementNode::new(ElementData::new(Role::TextField).with_value(&long_value)),
        );

        let renderer = TreeRenderer::new(false);
        let output = renderer.render(&tree);

        assert!(output.contains("..."));
        assert!(!output.contains(&long_value));
    }

    #[test]
    fn nested_indentation() {
        let tree = ElementTree::new(
            "App",
            ElementNode::new(ElementData::new(Role::Window)).with_children(vec![ElementNode::new(
                ElementData::new(Role::Group),
            )
            .with_children(vec![ElementNode::new(
                ElementData::new(Role::Button).with_name("Deep"),
            )])]),
        );

        let renderer = TreeRenderer::new(false);
        let output = renderer.render(&tree);
        let lines: Vec<&str> = output.lines().collect();

        assert_eq!(lines[1], "window");
        assert_eq!(lines[2], "  group");
        assert_eq!(lines[3], "    button \"Deep\"");
    }

    #[test]
    fn omits_empty_name_and_value() {
        let tree = ElementTree::new("App", ElementNode::new(ElementData::new(Role::Group)));

        let renderer = TreeRenderer::new(false);
        let output = renderer.render(&tree);

        assert_eq!(output, "app: App\ngroup");
    }

    #[test]
    fn renders_bounds_relative() {
        let tree = ElementTree::new(
            "App",
            ElementNode::new(
                ElementData::new(Role::Window)
                    .with_name("Main")
                    .with_bounds(Rect::new(100.0, 200.0, 800.0, 600.0)),
            )
            .with_children(vec![ElementNode::new(
                ElementData::new(Role::Button)
                    .with_name("OK")
                    .with_reference(ElementRef::new(1))
                    .with_bounds(Rect::new(150.0, 250.0, 80.0, 30.0)),
            )]),
        )
        .with_window_bounds(Rect::new(100.0, 200.0, 800.0, 600.0));

        let renderer = TreeRenderer::new(false);
        let output = renderer.render(&tree);
        let lines: Vec<&str> = output.lines().collect();

        // Header includes window bounds
        assert_eq!(lines[0], "app: App  window: [100,200 800x600]");
        // Window itself should be at 0,0 relative to itself
        assert_eq!(lines[1], "window \"Main\" (0,0 800x600)");
        // Button at 150,250 screen -> 50,50 window-relative
        assert_eq!(lines[2], "  button @e1 \"OK\" (50,50 80x30)");
    }

    #[test]
    fn renders_bounds_absolute() {
        let tree = ElementTree::new(
            "App",
            ElementNode::new(
                ElementData::new(Role::Window)
                    .with_name("Main")
                    .with_bounds(Rect::new(100.0, 200.0, 800.0, 600.0)),
            )
            .with_children(vec![ElementNode::new(
                ElementData::new(Role::Button)
                    .with_name("OK")
                    .with_reference(ElementRef::new(1))
                    .with_bounds(Rect::new(150.0, 250.0, 80.0, 30.0)),
            )]),
        );

        let renderer = TreeRenderer::new(false);
        let output = renderer.render(&tree);
        let lines: Vec<&str> = output.lines().collect();

        // No windowBounds -> absolute coordinates
        assert_eq!(lines[1], "window \"Main\" (100,200 800x600)");
        assert_eq!(lines[2], "  button @e1 \"OK\" (150,250 80x30)");
    }

    #[test]
    fn omits_missing_bounds() {
        let tree = ElementTree::new(
            "App",
            ElementNode::new(
                ElementData::new(Role::Button)
                    .with_name("OK")
                    .with_reference(ElementRef::new(1)),
            ),
        );

        let renderer = TreeRenderer::new(false);
        let output = renderer.render(&tree);

        assert!(!output.contains('('));
        assert!(output.contains("button @e1 \"OK\""));
    }

    #[test]
    fn shows_disabled_state() {
        let tree = ElementTree::new(
            "App",
            ElementNode::new(
                ElementData::new(Role::Button)
                    .with_name("OK")
                    .with_reference(ElementRef::new(1))
                    .with_enabled(false),
            ),
        );

        let renderer = TreeRenderer::new(false);
        let output = renderer.render(&tree);

        assert!(output.contains("disabled"));
        assert!(output.contains("button @e1 \"OK\" disabled"));
    }

    #[test]
    fn shows_focused_and_selected() {
        let tree = ElementTree::new(
            "App",
            ElementNode::new(
                ElementData::new(Role::TextField)
                    .with_name("Name")
                    .with_reference(ElementRef::new(1))
                    .with_focused(true)
                    .with_selected(true),
            ),
        );

        let renderer = TreeRenderer::new(false);
        let output = renderer.render(&tree);

        assert!(output.contains("focused selected"));
    }

    #[test]
    fn enabled_true_is_not_shown() {
        let tree = ElementTree::new(
            "App",
            ElementNode::new(
                ElementData::new(Role::Button)
                    .with_name("OK")
                    .with_reference(ElementRef::new(1))
                    .with_enabled(true),
            ),
        );

        let renderer = TreeRenderer::new(false);
        let output = renderer.render(&tree);

        assert!(!output.contains("enabled"));
        assert!(output.contains("button @e1 \"OK\""));
    }

    #[test]
    fn verbose_shows_description() {
        let tree = ElementTree::new(
            "App",
            ElementNode::new(
                ElementData::new(Role::Button)
                    .with_name("OK")
                    .with_reference(ElementRef::new(1))
                    .with_description("Confirms the action"),
            ),
        );

        let renderer = TreeRenderer::new(false);
        let output = renderer.render(&tree);
        assert!(!output.contains("desc="));

        let renderer = TreeRenderer::new(true);
        let output = renderer.render(&tree);
        assert!(output.contains("desc=\"Confirms the action\""));
    }

    #[test]
    fn verbose_shows_native_role_and_identifier() {
        let tree = ElementTree::new(
            "App",
            ElementNode::new(
                ElementData::new(Role::Button)
                    .with_name("OK")
                    .with_reference(ElementRef::new(1))
                    .with_native_role("AXButton")
                    .with_identifier("submit-btn"),
            ),
        );

        let renderer = TreeRenderer::new(false);
        let output = renderer.render(&tree);
        assert!(!output.contains("native_role="));
        assert!(!output.contains("id="));

        let renderer = TreeRenderer::new(true);
        let output = renderer.render(&tree);
        assert!(output.contains("native_role=AXButton"));
        assert!(output.contains("id=\"submit-btn\""));
    }

    #[test]
    fn verbose_hides_uid_and_sig_by_default() {
        let tree = ElementTree::new(
            "App",
            ElementNode::new(
                ElementData::new(Role::Button)
                    .with_name("OK")
                    .with_reference(ElementRef::new(1)),
            ),
        );

        let renderer = TreeRenderer::new(false);
        let output = renderer.render(&tree);
        assert!(!output.contains("uid="));
        assert!(!output.contains("sig="));
    }

    #[test]
    fn verbose_shows_uid() {
        let mut data = ElementData::new(Role::Button).with_name("OK");
        data.uid = Some(7);
        let tree = ElementTree::new(
            "App",
            ElementNode::new(data.with_reference(ElementRef::new(1))),
        );

        let renderer = TreeRenderer::new(true);
        let output = renderer.render(&tree);
        assert!(output.contains("uid=7"));
    }

    #[test]
    fn verbose_shows_signature() {
        let mut data = ElementData::new(Role::Button).with_name("OK");
        data.signature = Some(0xdead_beef_cafe_babe);
        let tree = ElementTree::new(
            "App",
            ElementNode::new(data.with_reference(ElementRef::new(1))),
        );

        let renderer = TreeRenderer::new(true);
        let output = renderer.render(&tree);
        assert!(output.contains("sig=0xdeadbeefcafebabe"));
    }
}
