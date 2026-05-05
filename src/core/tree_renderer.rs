/// Renders an ElementNode tree as indented text.
use crate::core::element_tree::ElementTree;
use crate::core::types::Rect;

pub struct TreeRenderer;

impl TreeRenderer {
    pub fn new() -> Self {
        Self
    }

    pub fn render(&self, tree: &ElementTree) -> String {
        let mut lines: Vec<String> = Vec::new();
        lines.push(format!("app: {}", tree.app));
        self.render_node(&tree.root, 0, tree.window_bounds.as_ref(), &mut lines);
        lines.join("\n")
    }

    fn render_node(
        &self,
        node: &crate::core::element_tree::ElementNode,
        indent: usize,
        window_origin: Option<&Rect>,
        lines: &mut Vec<String>,
    ) {
        let prefix = "  ".repeat(indent);
        let mut parts: Vec<String> = Vec::new();

        // Role (strip AX prefix, lowercase)
        let role = if node.role.starts_with("AX") {
            node.role[2..].to_lowercase()
        } else {
            node.role.clone()
        };
        parts.push(role);

        // Ref
        if let Some(r) = &node.r#ref {
            parts.push(r.to_string());
        }

        // Name
        if let Some(name) = &node.name {
            if !name.is_empty() {
                parts.push(format!("\"{name}\""));
            }
        }

        // Value (truncated for display)
        if let Some(value) = &node.value {
            if !value.is_empty() {
                let display = if value.len() > 80 {
                    format!("{}...", &value[..value.ceil_char_boundary(77)])
                } else {
                    value.clone()
                };
                parts.push(format!("value=\"{display}\""));
            }
        }

        // Bounds (window-relative when window bounds are available)
        if let Some(b) = &node.bounds {
            let display_x: i64;
            let display_y: i64;
            if let Some(w) = window_origin {
                display_x = (b.x - w.x).round() as i64;
                display_y = (b.y - w.y).round() as i64;
            } else {
                display_x = b.x.round() as i64;
                display_y = b.y.round() as i64;
            }
            parts.push(format!(
                "({display_x},{display_y} {}x{})",
                b.width.round() as i64,
                b.height.round() as i64
            ));
        }

        // Extra attributes (sorted by key)
        let mut sorted_attrs = node.attributes.clone();
        sorted_attrs.sort_by(|a, b| a.0.cmp(&b.0));
        for (key, val) in &sorted_attrs {
            parts.push(format!("{key}={val}"));
        }

        lines.push(format!("{prefix}{}", parts.join(" ")));

        for child in &node.children {
            self.render_node(child, indent + 1, window_origin, lines);
        }
    }
}

impl Default for TreeRenderer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::element_tree::{ElementNode, ElementRef, ElementTree};
    use crate::core::types::Rect;

    #[test]
    fn simple_tree() {
        let tree = ElementTree::new(
            "TestApp",
            ElementNode::new("AXWindow")
                .with_name("Main Window")
                .with_children(vec![
                    ElementNode::new("AXButton")
                        .with_name("OK")
                        .with_ref(ElementRef::new(1)),
                    ElementNode::new("AXTextField")
                        .with_name("Name")
                        .with_value("hello")
                        .with_ref(ElementRef::new(2)),
                ]),
        );

        let renderer = TreeRenderer::new();
        let output = renderer.render(&tree);
        let lines: Vec<&str> = output.lines().collect();

        assert_eq!(lines[0], "app: TestApp");
        assert_eq!(lines[1], "window \"Main Window\"");
        assert_eq!(lines[2], "  button @e1 \"OK\"");
        assert_eq!(lines[3], "  textfield @e2 \"Name\" value=\"hello\"");
    }

    #[test]
    fn strips_ax_prefix() {
        let tree = ElementTree::new("App", ElementNode::new("AXSplitGroup"));

        let renderer = TreeRenderer::new();
        let output = renderer.render(&tree);

        assert!(output.contains("splitgroup"));
        assert!(!output.contains("AXSplitGroup"));
    }

    #[test]
    fn truncates_long_values() {
        let long_value: String = "x".repeat(100);
        let tree = ElementTree::new(
            "App",
            ElementNode::new("AXTextField").with_value(&long_value),
        );

        let renderer = TreeRenderer::new();
        let output = renderer.render(&tree);

        assert!(output.contains("..."));
        assert!(!output.contains(&long_value));
    }

    #[test]
    fn nested_indentation() {
        let tree = ElementTree::new(
            "App",
            ElementNode::new("AXWindow").with_children(vec![ElementNode::new("AXGroup")
                .with_children(vec![ElementNode::new("AXButton").with_name("Deep")])]),
        );

        let renderer = TreeRenderer::new();
        let output = renderer.render(&tree);
        let lines: Vec<&str> = output.lines().collect();

        assert_eq!(lines[1], "window");
        assert_eq!(lines[2], "  group");
        assert_eq!(lines[3], "    button \"Deep\"");
    }

    #[test]
    fn omits_empty_name_and_value() {
        let tree = ElementTree::new("App", ElementNode::new("AXGroup"));

        let renderer = TreeRenderer::new();
        let output = renderer.render(&tree);

        assert_eq!(output, "app: App\ngroup");
    }

    #[test]
    fn renders_bounds_relative() {
        let tree = ElementTree::new(
            "App",
            ElementNode::new("AXWindow")
                .with_name("Main")
                .with_bounds(Rect::new(100.0, 200.0, 800.0, 600.0))
                .with_children(vec![ElementNode::new("AXButton")
                    .with_name("OK")
                    .with_ref(ElementRef::new(1))
                    .with_bounds(Rect::new(150.0, 250.0, 80.0, 30.0))]),
        )
        .with_window_bounds(Rect::new(100.0, 200.0, 800.0, 600.0));

        let renderer = TreeRenderer::new();
        let output = renderer.render(&tree);
        let lines: Vec<&str> = output.lines().collect();

        // Window itself should be at 0,0 relative to itself
        assert_eq!(lines[1], "window \"Main\" (0,0 800x600)");
        // Button at 150,250 screen -> 50,50 window-relative
        assert_eq!(lines[2], "  button @e1 \"OK\" (50,50 80x30)");
    }

    #[test]
    fn renders_bounds_absolute() {
        let tree = ElementTree::new(
            "App",
            ElementNode::new("AXWindow")
                .with_name("Main")
                .with_bounds(Rect::new(100.0, 200.0, 800.0, 600.0))
                .with_children(vec![ElementNode::new("AXButton")
                    .with_name("OK")
                    .with_ref(ElementRef::new(1))
                    .with_bounds(Rect::new(150.0, 250.0, 80.0, 30.0))]),
        );

        let renderer = TreeRenderer::new();
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
            ElementNode::new("AXButton")
                .with_name("OK")
                .with_ref(ElementRef::new(1)),
        );

        let renderer = TreeRenderer::new();
        let output = renderer.render(&tree);

        assert!(!output.contains('('));
        assert!(output.contains("button @e1 \"OK\""));
    }
}
