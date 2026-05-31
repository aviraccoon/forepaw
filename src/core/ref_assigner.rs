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
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Walk the tree, assigning refs to interactive elements.
    /// Returns a new tree with refs populated and a ref lookup table.
    #[must_use]
    pub fn assign(&self, root: &ElementNode, interactive_only: bool) -> RefAssignment {
        let mut counter: i32 = 1;
        let mut refs = HashMap::new();
        let new_root = Self::walk(root, &mut counter, &mut refs, interactive_only);
        RefAssignment {
            root: new_root,
            refs,
        }
    }

    fn walk(
        node: &ElementNode,
        counter: &mut i32,
        refs: &mut HashMap<ElementRef, ElementRefInfo>,
        interactive_only: bool,
    ) -> ElementNode {
        let mut new_data = node.data.clone();

        if node.is_interactive() {
            let element_ref = ElementRef::new(*counter);
            new_data.reference = Some(element_ref);
            refs.insert(
                element_ref,
                ElementRefInfo::new(node.data.role, node.data.name.clone()),
            );
            *counter += 1;
        } else {
            new_data.reference = node.data.reference;
        }

        let children: Vec<ElementNode> = if interactive_only {
            node.children
                .iter()
                .filter_map(|child| {
                    let walked = Self::walk(child, counter, refs, true);
                    if walked.data.reference.is_some() || !walked.children.is_empty() {
                        Some(walked)
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            node.children
                .iter()
                .map(|child| Self::walk(child, counter, refs, false))
                .collect()
        };

        ElementNode {
            data: new_data,
            children,
        }
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
    use crate::core::element_tree::ElementData;
    use crate::core::role::Role;

    fn make_tree() -> ElementNode {
        ElementNode::new(ElementData::new(Role::Window).with_name("Test Window")).with_children(
            vec![
                ElementNode::new(ElementData::new(Role::Group)).with_children(vec![
                    ElementNode::new(ElementData::new(Role::Button).with_name("OK")),
                    ElementNode::new(ElementData::new(Role::Button).with_name("Cancel")),
                ]),
                ElementNode::new(ElementData::new(Role::TextField).with_name("Name")),
            ],
        )
    }

    #[test]
    fn assigns_refs_depth_first() {
        let tree = make_tree();
        let assigner = RefAssigner::new();
        let result = assigner.assign(&tree, false);

        assert_eq!(
            result.root.children[0].children[0].data.reference,
            Some(ElementRef::new(1))
        );
        assert_eq!(
            result.root.children[0].children[1].data.reference,
            Some(ElementRef::new(2))
        );
        assert_eq!(
            result.root.children[1].data.reference,
            Some(ElementRef::new(3))
        );
        assert!(result.root.data.reference.is_none());
    }

    #[test]
    fn interactive_only_prunes() {
        let tree = ElementNode::new(ElementData::new(Role::Window)).with_children(vec![
            ElementNode::new(ElementData::new(Role::Group)).with_children(vec![
                ElementNode::new(ElementData::new(Role::StaticText).with_name("Label")),
                ElementNode::new(ElementData::new(Role::Button).with_name("OK")),
            ]),
            ElementNode::new(ElementData::new(Role::StaticText).with_name("Footer")),
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
