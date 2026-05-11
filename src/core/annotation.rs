/// Data model for screenshot annotations.
use crate::core::element_tree::{ElementNode, ElementRef};
use crate::core::types::Rect;

/// A single element annotation: a labeled marker on a screenshot.
#[derive(Debug, Clone)]
pub struct Annotation {
    pub r#ref: ElementRef,
    pub display_number: usize,
    pub role: String,
    pub name: Option<String>,
    pub bounds: Rect,
}

impl Annotation {
    pub fn new(
        r#ref: ElementRef,
        display_number: usize,
        role: impl Into<String>,
        name: Option<String>,
        bounds: Rect,
    ) -> Self {
        Self {
            r#ref,
            display_number,
            role: role.into(),
            name,
            bounds,
        }
    }

    /// Short role label for display (strips "AX" prefix).
    pub fn short_role(&self) -> &str {
        if let Some(stripped) = self.role.strip_prefix("AX") {
            stripped
        } else {
            &self.role
        }
    }
}

/// Visual style for screenshot annotations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationStyle {
    /// Small numbered badges at element positions.
    Badges,
    /// Colored bounding boxes with role + name labels.
    Labeled,
    /// Dims everything except annotated elements.
    Spotlight,
}

impl std::str::FromStr for AnnotationStyle {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "badges" => Ok(Self::Badges),
            "labeled" => Ok(Self::Labeled),
            "spotlight" => Ok(Self::Spotlight),
            _ => Err(()),
        }
    }
}

impl AnnotationStyle {
    pub fn all() -> &'static [AnnotationStyle] {
        &[Self::Badges, Self::Labeled, Self::Spotlight]
    }
}

/// Category for color-coding elements by type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationCategory {
    Button,
    TextInput,
    Selection,
    Navigation,
    Other,
}

impl AnnotationCategory {
    pub fn from_role(role: &str) -> Self {
        match role {
            "AXButton" | "AXMenuButton" | "AXDockItem" | "AXIncrementor" => Self::Button,
            "AXTextField" | "AXTextArea" => Self::TextInput,
            "AXCheckBox" | "AXRadioButton" | "AXSwitch" | "AXComboBox" | "AXPopUpButton"
            | "AXSlider" | "AXColorWell" => Self::Selection,
            "AXLink" | "AXTab" | "AXMenuItem" | "AXTreeItem" => Self::Navigation,
            _ => Self::Other,
        }
    }
}

/// Collects annotations from an element tree.
pub struct AnnotationCollector;

impl AnnotationCollector {
    pub fn new() -> Self {
        Self
    }

    /// Collect annotations for all interactive elements with bounds.
    pub fn collect(&self, root: &ElementNode, window_bounds: Rect) -> Vec<Annotation> {
        let mut annotations = Vec::new();
        let mut display_number: usize = 1;
        self.walk(root, &mut annotations, &mut display_number, window_bounds);
        annotations
    }

    fn walk(
        &self,
        node: &ElementNode,
        annotations: &mut Vec<Annotation>,
        display_number: &mut usize,
        window_bounds: Rect,
    ) {
        if let Some(r) = &node.r#ref {
            if let Some(bounds) = &node.bounds {
                if node.is_interactive() {
                    // Convert to window-relative coordinates
                    let rel = Rect::new(
                        bounds.x - window_bounds.x,
                        bounds.y - window_bounds.y,
                        bounds.width,
                        bounds.height,
                    );

                    // Only include elements that overlap the window area
                    if rel.x + rel.width > 0.0
                        && rel.y + rel.height > 0.0
                        && rel.x < window_bounds.width
                        && rel.y < window_bounds.height
                    {
                        annotations.push(Annotation::new(
                            *r,
                            *display_number,
                            &node.role,
                            node.name.clone(),
                            rel,
                        ));
                        *display_number += 1;
                    }
                }
            }
        }

        for child in &node.children {
            self.walk(child, annotations, display_number, window_bounds);
        }
    }
}

impl Default for AnnotationCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Formats the text legend for annotations.
pub struct AnnotationLegend;

impl AnnotationLegend {
    pub fn new() -> Self {
        Self
    }

    /// Generate a compact legend mapping display numbers to refs and element info.
    pub fn format(&self, annotations: &[Annotation]) -> String {
        annotations
            .iter()
            .map(|a| {
                let name = a
                    .name
                    .as_ref()
                    .map(|n| {
                        if n.is_empty() {
                            String::new()
                        } else {
                            format!(" \"{n}\"")
                        }
                    })
                    .unwrap_or_default();
                format!(
                    "[{}] {} {}{}",
                    a.display_number,
                    a.r#ref,
                    a.short_role(),
                    name
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl Default for AnnotationLegend {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::element_tree::ElementRef;

    #[test]
    fn button_roles() {
        assert_eq!(
            AnnotationCategory::from_role("AXButton"),
            AnnotationCategory::Button
        );
        assert_eq!(
            AnnotationCategory::from_role("AXMenuButton"),
            AnnotationCategory::Button
        );
        assert_eq!(
            AnnotationCategory::from_role("AXDockItem"),
            AnnotationCategory::Button
        );
        assert_eq!(
            AnnotationCategory::from_role("AXIncrementor"),
            AnnotationCategory::Button
        );
    }

    #[test]
    fn text_input_roles() {
        assert_eq!(
            AnnotationCategory::from_role("AXTextField"),
            AnnotationCategory::TextInput
        );
        assert_eq!(
            AnnotationCategory::from_role("AXTextArea"),
            AnnotationCategory::TextInput
        );
    }

    #[test]
    fn selection_roles() {
        assert_eq!(
            AnnotationCategory::from_role("AXCheckBox"),
            AnnotationCategory::Selection
        );
        assert_eq!(
            AnnotationCategory::from_role("AXRadioButton"),
            AnnotationCategory::Selection
        );
        assert_eq!(
            AnnotationCategory::from_role("AXSlider"),
            AnnotationCategory::Selection
        );
        assert_eq!(
            AnnotationCategory::from_role("AXComboBox"),
            AnnotationCategory::Selection
        );
        assert_eq!(
            AnnotationCategory::from_role("AXPopUpButton"),
            AnnotationCategory::Selection
        );
        assert_eq!(
            AnnotationCategory::from_role("AXSwitch"),
            AnnotationCategory::Selection
        );
        assert_eq!(
            AnnotationCategory::from_role("AXColorWell"),
            AnnotationCategory::Selection
        );
    }

    #[test]
    fn navigation_roles() {
        assert_eq!(
            AnnotationCategory::from_role("AXLink"),
            AnnotationCategory::Navigation
        );
        assert_eq!(
            AnnotationCategory::from_role("AXTab"),
            AnnotationCategory::Navigation
        );
        assert_eq!(
            AnnotationCategory::from_role("AXMenuItem"),
            AnnotationCategory::Navigation
        );
        assert_eq!(
            AnnotationCategory::from_role("AXTreeItem"),
            AnnotationCategory::Navigation
        );
    }

    #[test]
    fn unknown_roles() {
        assert_eq!(
            AnnotationCategory::from_role("AXGroup"),
            AnnotationCategory::Other
        );
        assert_eq!(
            AnnotationCategory::from_role("AXImage"),
            AnnotationCategory::Other
        );
        assert_eq!(
            AnnotationCategory::from_role("AXUnknown"),
            AnnotationCategory::Other
        );
    }

    #[test]
    fn short_role_strips_prefix() {
        let a = Annotation::new(
            ElementRef::new(5),
            1,
            "AXButton",
            Some("Save".into()),
            Rect::new(0.0, 0.0, 100.0, 30.0),
        );
        assert_eq!(a.short_role(), "Button");
    }

    #[test]
    fn short_role_preserves_non_ax() {
        let a = Annotation::new(
            ElementRef::new(1),
            1,
            "CustomRole",
            None,
            Rect::new(0.0, 0.0, 50.0, 50.0),
        );
        assert_eq!(a.short_role(), "CustomRole");
    }

    #[test]
    fn style_from_str() {
        assert_eq!("badges".parse(), Ok(AnnotationStyle::Badges));
        assert_eq!("labeled".parse(), Ok(AnnotationStyle::Labeled));
        assert_eq!("spotlight".parse(), Ok(AnnotationStyle::Spotlight));
        assert_eq!("invalid".parse::<AnnotationStyle>(), Err(()));
    }

    #[test]
    fn collects_interactive() {
        let window_bounds = Rect::new(100.0, 50.0, 800.0, 600.0);
        let root = ElementNode::new("AXWindow").with_children(vec![
            ElementNode::new("AXButton")
                .with_name("Save")
                .with_bounds(Rect::new(200.0, 100.0, 80.0, 30.0))
                .with_ref(ElementRef::new(1)),
            ElementNode::new("AXTextField")
                .with_name("Search")
                .with_bounds(Rect::new(300.0, 100.0, 200.0, 25.0))
                .with_ref(ElementRef::new(2)),
        ]);

        let collector = AnnotationCollector::new();
        let annotations = collector.collect(&root, window_bounds);

        assert_eq!(annotations.len(), 2);
        assert_eq!(annotations[0].display_number, 1);
        assert_eq!(annotations[0].r#ref, ElementRef::new(1));
        assert_eq!(annotations[0].name.as_deref(), Some("Save"));
        assert_eq!(annotations[1].display_number, 2);
        assert_eq!(annotations[1].r#ref, ElementRef::new(2));
    }

    #[test]
    fn window_relative_coords() {
        let window_bounds = Rect::new(100.0, 50.0, 800.0, 600.0);
        let root = ElementNode::new("AXWindow").with_children(vec![ElementNode::new("AXButton")
            .with_name("OK")
            .with_bounds(Rect::new(250.0, 150.0, 60.0, 30.0))
            .with_ref(ElementRef::new(1))]);

        let collector = AnnotationCollector::new();
        let annotations = collector.collect(&root, window_bounds);

        assert_eq!(annotations.len(), 1);
        // 250 - 100 = 150, 150 - 50 = 100
        assert!((annotations[0].bounds.x - 150.0).abs() < f64::EPSILON);
        assert!((annotations[0].bounds.y - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn skips_no_bounds() {
        let window_bounds = Rect::new(100.0, 50.0, 800.0, 600.0);
        let root = ElementNode::new("AXWindow").with_children(vec![ElementNode::new("AXButton")
            .with_name("Ghost")
            .with_ref(ElementRef::new(1))]);

        let collector = AnnotationCollector::new();
        let annotations = collector.collect(&root, window_bounds);
        assert!(annotations.is_empty());
    }

    #[test]
    fn skips_non_interactive() {
        let window_bounds = Rect::new(100.0, 50.0, 800.0, 600.0);
        let root =
            ElementNode::new("AXWindow").with_children(vec![ElementNode::new("AXStaticText")
                .with_name("Label")
                .with_bounds(Rect::new(200.0, 100.0, 80.0, 20.0))]);

        let collector = AnnotationCollector::new();
        let annotations = collector.collect(&root, window_bounds);
        assert!(annotations.is_empty());
    }

    #[test]
    fn skips_off_screen() {
        let window_bounds = Rect::new(100.0, 50.0, 800.0, 600.0);
        let root = ElementNode::new("AXWindow").with_children(vec![
            // Entirely to the left
            ElementNode::new("AXButton")
                .with_name("Hidden")
                .with_bounds(Rect::new(0.0, 100.0, 50.0, 30.0))
                .with_ref(ElementRef::new(1)),
            // Entirely below
            ElementNode::new("AXButton")
                .with_name("Below")
                .with_bounds(Rect::new(200.0, 700.0, 80.0, 30.0))
                .with_ref(ElementRef::new(2)),
            // Visible
            ElementNode::new("AXButton")
                .with_name("Visible")
                .with_bounds(Rect::new(200.0, 100.0, 80.0, 30.0))
                .with_ref(ElementRef::new(3)),
        ]);

        let collector = AnnotationCollector::new();
        let annotations = collector.collect(&root, window_bounds);

        assert_eq!(annotations.len(), 1);
        assert_eq!(annotations[0].name.as_deref(), Some("Visible"));
    }

    #[test]
    fn sequential_display_numbers() {
        let window_bounds = Rect::new(100.0, 50.0, 800.0, 600.0);
        let root = ElementNode::new("AXWindow").with_children(vec![
            ElementNode::new("AXButton")
                .with_name("A")
                .with_bounds(Rect::new(200.0, 100.0, 80.0, 30.0))
                .with_ref(ElementRef::new(5)),
            ElementNode::new("AXButton")
                .with_name("B")
                .with_bounds(Rect::new(300.0, 100.0, 80.0, 30.0))
                .with_ref(ElementRef::new(10)),
        ]);

        let collector = AnnotationCollector::new();
        let annotations = collector.collect(&root, window_bounds);

        assert_eq!(annotations[0].display_number, 1);
        assert_eq!(annotations[0].r#ref, ElementRef::new(5));
        assert_eq!(annotations[1].display_number, 2);
        assert_eq!(annotations[1].r#ref, ElementRef::new(10));
    }

    #[test]
    fn legend_with_names() {
        let annotations = vec![
            Annotation::new(
                ElementRef::new(5),
                1,
                "AXButton",
                Some("Save".into()),
                Rect::new(0.0, 0.0, 80.0, 30.0),
            ),
            Annotation::new(
                ElementRef::new(8),
                2,
                "AXTextField",
                Some("Search".into()),
                Rect::new(0.0, 0.0, 200.0, 25.0),
            ),
        ];
        let legend = AnnotationLegend::new().format(&annotations);
        assert_eq!(
            legend,
            "[1] @e5 Button \"Save\"\n[2] @e8 TextField \"Search\""
        );
    }

    #[test]
    fn legend_omits_empty_names() {
        let annotations = vec![
            Annotation::new(
                ElementRef::new(1),
                1,
                "AXButton",
                None,
                Rect::new(0.0, 0.0, 80.0, 30.0),
            ),
            Annotation::new(
                ElementRef::new(2),
                2,
                "AXButton",
                Some(String::new()),
                Rect::new(0.0, 0.0, 80.0, 30.0),
            ),
        ];
        let legend = AnnotationLegend::new().format(&annotations);
        assert_eq!(legend, "[1] @e1 Button\n[2] @e2 Button");
    }
}
