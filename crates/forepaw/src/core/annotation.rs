//! Data model for screenshot annotations.
use crate::core::element_tree::{ElementNode, ElementRef};
use crate::core::role::Role;
use crate::core::types::Rect;

/// A single element annotation: a labeled marker on a screenshot.
#[derive(Debug, Clone)]
pub struct Annotation {
    /// The element ref (e.g. @e3).
    pub reference: ElementRef,
    /// Sequential display number for annotation labels (1-based).
    pub display_number: usize,
    /// The element's role.
    pub role: Role,
    /// The element's accessible name, if any.
    pub name: Option<String>,
    /// Element bounds in window-relative coordinates.
    pub bounds: Rect,
}

impl Annotation {
    /// Create a new annotation.
    #[must_use]
    pub fn new(
        reference: ElementRef,
        display_number: usize,
        role: Role,
        name: Option<String>,
        bounds: Rect,
    ) -> Self {
        Self {
            reference,
            display_number,
            role,
            name,
            bounds,
        }
    }

    /// Short role label for display (title-case, no prefix).
    #[must_use]
    pub fn short_role(&self) -> &'static str {
        self.role.short_name()
    }
}

/// Visual style for screenshot annotations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
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
    /// Return all available annotation styles.
    #[must_use]
    pub fn all() -> &'static [Self] {
        &[Self::Badges, Self::Labeled, Self::Spotlight]
    }
}

/// Collects annotations from an element tree.
#[derive(Debug)]
pub struct AnnotationCollector;

impl AnnotationCollector {
    /// Create a new collector.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Collect annotations for all interactive elements with bounds.
    #[must_use]
    pub fn collect(&self, root: &ElementNode, window_bounds: Rect) -> Vec<Annotation> {
        let mut annotations = Vec::new();
        let mut display_number: usize = 1;
        Self::walk(root, &mut annotations, &mut display_number, window_bounds);
        annotations
    }

    fn walk(
        node: &ElementNode,
        annotations: &mut Vec<Annotation>,
        display_number: &mut usize,
        window_bounds: Rect,
    ) {
        if let Some(r) = &node.data.reference {
            if let Some(bounds) = &node.data.bounds {
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
                            node.data.role,
                            node.data.name.clone(),
                            rel,
                        ));
                        *display_number += 1;
                    }
                }
            }
        }

        for child in &node.children {
            Self::walk(child, annotations, display_number, window_bounds);
        }
    }
}

impl Default for AnnotationCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Formats the text legend for annotations.
#[derive(Debug)]
pub struct AnnotationLegend;

impl AnnotationLegend {
    /// Create a new legend formatter.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Generate a compact legend mapping display numbers to refs and element info.
    #[must_use]
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
                    a.reference,
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
    use crate::core::element_tree::ElementData;
    use crate::core::element_tree::ElementRef;
    use crate::core::role::AnnotationCategory;

    #[test]
    fn button_roles() {
        assert_eq!(
            Role::Button.annotation_category(),
            AnnotationCategory::Button
        );
        assert_eq!(
            Role::MenuButton.annotation_category(),
            AnnotationCategory::Button
        );
        assert_eq!(
            Role::DockItem.annotation_category(),
            AnnotationCategory::Button
        );
        assert_eq!(
            Role::Incrementor.annotation_category(),
            AnnotationCategory::Button
        );
    }

    #[test]
    fn text_input_roles() {
        assert_eq!(
            Role::TextField.annotation_category(),
            AnnotationCategory::TextInput
        );
        assert_eq!(
            Role::TextArea.annotation_category(),
            AnnotationCategory::TextInput
        );
    }

    #[test]
    fn selection_roles() {
        assert_eq!(
            Role::CheckBox.annotation_category(),
            AnnotationCategory::Selection
        );
        assert_eq!(
            Role::RadioButton.annotation_category(),
            AnnotationCategory::Selection
        );
        assert_eq!(
            Role::Slider.annotation_category(),
            AnnotationCategory::Selection
        );
        assert_eq!(
            Role::ComboBox.annotation_category(),
            AnnotationCategory::Selection
        );
        assert_eq!(
            Role::PopUpButton.annotation_category(),
            AnnotationCategory::Selection
        );
        assert_eq!(
            Role::Switch.annotation_category(),
            AnnotationCategory::Selection
        );
        assert_eq!(
            Role::ColorWell.annotation_category(),
            AnnotationCategory::Selection
        );
    }

    #[test]
    fn navigation_roles() {
        assert_eq!(
            Role::Link.annotation_category(),
            AnnotationCategory::Navigation
        );
        assert_eq!(
            Role::Tab.annotation_category(),
            AnnotationCategory::Navigation
        );
        assert_eq!(
            Role::MenuItem.annotation_category(),
            AnnotationCategory::Navigation
        );
        assert_eq!(
            Role::TreeItem.annotation_category(),
            AnnotationCategory::Navigation
        );
    }

    #[test]
    fn unknown_roles() {
        assert_eq!(Role::Group.annotation_category(), AnnotationCategory::Other);
        assert_eq!(Role::Image.annotation_category(), AnnotationCategory::Other);
        assert_eq!(
            Role::Unknown.annotation_category(),
            AnnotationCategory::Other
        );
    }

    #[test]
    fn short_role_returns_name() {
        let a = Annotation::new(
            ElementRef::new(5),
            1,
            Role::Button,
            Some("Save".into()),
            Rect::new(0.0, 0.0, 100.0, 30.0),
        );
        assert_eq!(a.short_role(), "Button");
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
        let root = ElementNode::new(ElementData::new(Role::Window)).with_children(vec![
            ElementNode::new(
                ElementData::new(Role::Button)
                    .with_name("Save")
                    .with_bounds(Rect::new(200.0, 100.0, 80.0, 30.0))
                    .with_reference(ElementRef::new(1)),
            ),
            ElementNode::new(
                ElementData::new(Role::TextField)
                    .with_name("Search")
                    .with_bounds(Rect::new(300.0, 100.0, 200.0, 25.0))
                    .with_reference(ElementRef::new(2)),
            ),
        ]);

        let collector = AnnotationCollector::new();
        let annotations = collector.collect(&root, window_bounds);

        assert_eq!(annotations.len(), 2);
        assert_eq!(annotations[0].display_number, 1);
        assert_eq!(annotations[0].reference, ElementRef::new(1));
        assert_eq!(annotations[0].name.as_deref(), Some("Save"));
        assert_eq!(annotations[1].display_number, 2);
        assert_eq!(annotations[1].reference, ElementRef::new(2));
    }

    #[test]
    fn window_relative_coords() {
        let window_bounds = Rect::new(100.0, 50.0, 800.0, 600.0);
        let root =
            ElementNode::new(ElementData::new(Role::Window)).with_children(vec![ElementNode::new(
                ElementData::new(Role::Button)
                    .with_name("OK")
                    .with_bounds(Rect::new(250.0, 150.0, 60.0, 30.0))
                    .with_reference(ElementRef::new(1)),
            )]);

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
        let root =
            ElementNode::new(ElementData::new(Role::Window)).with_children(vec![ElementNode::new(
                ElementData::new(Role::Button)
                    .with_name("Ghost")
                    .with_reference(ElementRef::new(1)),
            )]);

        let collector = AnnotationCollector::new();
        let annotations = collector.collect(&root, window_bounds);
        assert!(annotations.is_empty());
    }

    #[test]
    fn skips_non_interactive() {
        let window_bounds = Rect::new(100.0, 50.0, 800.0, 600.0);
        let root =
            ElementNode::new(ElementData::new(Role::Window)).with_children(vec![ElementNode::new(
                ElementData::new(Role::StaticText)
                    .with_name("Label")
                    .with_bounds(Rect::new(200.0, 100.0, 80.0, 20.0)),
            )]);

        let collector = AnnotationCollector::new();
        let annotations = collector.collect(&root, window_bounds);
        assert!(annotations.is_empty());
    }

    #[test]
    fn skips_off_screen() {
        let window_bounds = Rect::new(100.0, 50.0, 800.0, 600.0);
        let root = ElementNode::new(ElementData::new(Role::Window)).with_children(vec![
            // Entirely to the left
            ElementNode::new(
                ElementData::new(Role::Button)
                    .with_name("Hidden")
                    .with_bounds(Rect::new(0.0, 100.0, 50.0, 30.0))
                    .with_reference(ElementRef::new(1)),
            ),
            // Entirely below
            ElementNode::new(
                ElementData::new(Role::Button)
                    .with_name("Below")
                    .with_bounds(Rect::new(200.0, 700.0, 80.0, 30.0))
                    .with_reference(ElementRef::new(2)),
            ),
            // Visible
            ElementNode::new(
                ElementData::new(Role::Button)
                    .with_name("Visible")
                    .with_bounds(Rect::new(200.0, 100.0, 80.0, 30.0))
                    .with_reference(ElementRef::new(3)),
            ),
        ]);

        let collector = AnnotationCollector::new();
        let annotations = collector.collect(&root, window_bounds);

        assert_eq!(annotations.len(), 1);
        assert_eq!(annotations[0].name.as_deref(), Some("Visible"));
    }

    #[test]
    fn sequential_display_numbers() {
        let window_bounds = Rect::new(100.0, 50.0, 800.0, 600.0);
        let root = ElementNode::new(ElementData::new(Role::Window)).with_children(vec![
            ElementNode::new(
                ElementData::new(Role::Button)
                    .with_name("A")
                    .with_bounds(Rect::new(200.0, 100.0, 80.0, 30.0))
                    .with_reference(ElementRef::new(5)),
            ),
            ElementNode::new(
                ElementData::new(Role::Button)
                    .with_name("B")
                    .with_bounds(Rect::new(300.0, 100.0, 80.0, 30.0))
                    .with_reference(ElementRef::new(10)),
            ),
        ]);

        let collector = AnnotationCollector::new();
        let annotations = collector.collect(&root, window_bounds);

        assert_eq!(annotations[0].display_number, 1);
        assert_eq!(annotations[0].reference, ElementRef::new(5));
        assert_eq!(annotations[1].display_number, 2);
        assert_eq!(annotations[1].reference, ElementRef::new(10));
    }

    #[test]
    fn legend_with_names() {
        let annotations = vec![
            Annotation::new(
                ElementRef::new(5),
                1,
                Role::Button,
                Some("Save".into()),
                Rect::new(0.0, 0.0, 80.0, 30.0),
            ),
            Annotation::new(
                ElementRef::new(8),
                2,
                Role::TextField,
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
                Role::Button,
                None,
                Rect::new(0.0, 0.0, 80.0, 30.0),
            ),
            Annotation::new(
                ElementRef::new(2),
                2,
                Role::Button,
                Some(String::new()),
                Rect::new(0.0, 0.0, 80.0, 30.0),
            ),
        ];
        let legend = AnnotationLegend::new().format(&annotations);
        assert_eq!(legend, "[1] @e1 Button\n[2] @e2 Button");
    }
}
