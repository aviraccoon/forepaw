/// Assigns @e1, @e2, etc. to interactive elements in depth-first order.
use std::collections::HashMap;

use crate::core::element_tree::{ElementNode, ElementRef, ElementRefInfo};

/// Result of ref assignment.
pub struct RefAssignment {
    pub root: ElementNode,
    pub refs: HashMap<ElementRef, ElementRefInfo>,
}

/// Assigns refs to interactive elements in depth-first order.
pub struct RefAssigner;

impl RefAssigner {
    pub fn new() -> Self {
        Self
    }

    /// Walk the tree, assigning refs to interactive elements.
    /// Returns a new tree with refs populated and a ref lookup table.
    pub fn assign(&self, root: &ElementNode, interactive_only: bool) -> RefAssignment {
        let mut counter: i32 = 1;
        let mut refs = HashMap::new();
        let new_root = self.walk(root, &mut counter, &mut refs, interactive_only);
        RefAssignment {
            root: new_root,
            refs,
        }
    }

    fn walk(
        &self,
        node: &ElementNode,
        counter: &mut i32,
        refs: &mut HashMap<ElementRef, ElementRefInfo>,
        interactive_only: bool,
    ) -> ElementNode {
        let mut new_ref = None;

        if node.is_interactive() {
            let element_ref = ElementRef::new(*counter);
            new_ref = Some(element_ref);
            refs.insert(
                element_ref,
                ElementRefInfo::new(node.role.clone(), node.name.clone()),
            );
            *counter += 1;
        }

        let children: Vec<ElementNode> = if interactive_only {
            node.children
                .iter()
                .filter_map(|child| {
                    let walked = self.walk(child, counter, refs, true);
                    if walked.r#ref.is_some() || !walked.children.is_empty() {
                        Some(walked)
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            node.children
                .iter()
                .map(|child| self.walk(child, counter, refs, false))
                .collect()
        };

        let mut new_node = ElementNode::new(&node.role);
        new_node.name = node.name.clone();
        new_node.value = node.value.clone();
        new_node.r#ref = new_ref.or_else(|| node.r#ref);
        new_node.bounds = node.bounds;
        new_node.attributes = node.attributes.clone();
        new_node.children = children;
        new_node
    }
}

impl Default for RefAssigner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tree() -> ElementNode {
        ElementNode::new("AXWindow")
            .with_name("Test Window")
            .with_children(vec![
                ElementNode::new("AXGroup").with_children(vec![
                    ElementNode::new("AXButton").with_name("OK"),
                    ElementNode::new("AXButton").with_name("Cancel"),
                ]),
                ElementNode::new("AXTextField").with_name("Name"),
            ])
    }

    #[test]
    fn assigns_refs_depth_first() {
        let tree = make_tree();
        let assigner = RefAssigner::new();
        let result = assigner.assign(&tree, false);

        assert_eq!(
            result.root.children[0].children[0].r#ref,
            Some(ElementRef::new(1))
        );
        assert_eq!(
            result.root.children[0].children[1].r#ref,
            Some(ElementRef::new(2))
        );
        assert_eq!(result.root.children[1].r#ref, Some(ElementRef::new(3)));
        assert!(result.root.r#ref.is_none());
    }

    #[test]
    fn interactive_only_prunes() {
        let tree = ElementNode::new("AXWindow").with_children(vec![
            ElementNode::new("AXGroup").with_children(vec![
                ElementNode::new("AXStaticText").with_name("Label"),
                ElementNode::new("AXButton").with_name("OK"),
            ]),
            ElementNode::new("AXStaticText").with_name("Footer"),
        ]);

        let assigner = RefAssigner::new();
        let result = assigner.assign(&tree, true);

        // Footer (static text, no interactive children) should be pruned
        // Group stays because it has the button
        assert_eq!(result.root.children.len(), 1);
        assert_eq!(result.root.children[0].children.len(), 1);
    }

    #[test]
    fn ref_parsing() {
        assert_eq!(ElementRef::parse("@e3"), Some(ElementRef::new(3)));
        assert_eq!(ElementRef::parse("@e42"), Some(ElementRef::new(42)));
        assert_eq!(ElementRef::parse("e3"), None);
        assert_eq!(ElementRef::parse("@x3"), None);
        assert_eq!(ElementRef::parse(""), None);
    }
}
