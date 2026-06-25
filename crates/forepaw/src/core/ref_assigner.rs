//! Assigns uid, signature, and `@e` refs to every element in the tree.
//!
//! - `uid`: sequential counter on every element (within-snapshot stability).
//! - `signature`: content-based FNV-1a 64-bit hash (cross-snapshot identity).
//! - `@e` refs: only on interactive elements (agent action targeting).

use std::collections::HashMap;

use crate::core::element_tree::{ElementNode, ElementRef, ElementRefInfo};
use crate::core::signature::{element_signature, element_signature_with_bounds};

/// Result of ref assignment.
#[derive(Debug)]
pub struct RefAssignment {
    /// Root of the annotated tree.
    pub root: ElementNode,
    /// Map from refs to element info (role, name).
    pub refs: HashMap<ElementRef, ElementRefInfo>,
}

/// Assigns refs to interactive elements in depth-first order.
#[derive(Debug)]
pub struct RefAssigner;

impl RefAssigner {
    /// Create a new ref assigner.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Walk the tree, assigning uid, signature, and refs to every element.
    ///
    /// - Every element gets `uid` and `signature` (content-based hash).
    /// - Only interactive elements get `@e` refs.
    ///
    /// Returns a new tree with all fields populated and a ref lookup table.
    #[must_use]
    pub fn assign(&self, root: &ElementNode, interactive_only: bool) -> RefAssignment {
        let mut ref_counter: i32 = 1;
        let mut uid_counter: u64 = 0;
        let mut refs = HashMap::new();
        let new_root = Self::walk(
            root,
            &mut ref_counter,
            &mut uid_counter,
            &mut refs,
            interactive_only,
        );
        RefAssignment {
            root: new_root,
            refs,
        }
    }

    fn walk(
        node: &ElementNode,
        ref_counter: &mut i32,
        uid_counter: &mut u64,
        refs: &mut HashMap<ElementRef, ElementRefInfo>,
        interactive_only: bool,
    ) -> ElementNode {
        let mut new_data = node.data.clone();

        // Every element gets a uid and signature.
        *uid_counter += 1;
        new_data.uid = Some(*uid_counter);
        new_data.signature = Some(element_signature(
            new_data.role,
            new_data.name.as_deref(),
            new_data.identifier.as_deref(),
            new_data.native_role.as_deref(),
        ));
        new_data.signature_bounds = Some(element_signature_with_bounds(
            new_data.role,
            new_data.name.as_deref(),
            new_data.identifier.as_deref(),
            new_data.native_role.as_deref(),
            new_data.bounds,
        ));

        // Only interactive elements get @e refs.
        if node.is_interactive() {
            let element_ref = ElementRef::new(*ref_counter);
            new_data.reference = Some(element_ref);
            refs.insert(
                element_ref,
                ElementRefInfo::new(node.data.role, node.data.name.clone()),
            );
            *ref_counter += 1;
        } else {
            new_data.reference = node.data.reference;
        }

        let children: Vec<ElementNode> = if interactive_only {
            node.children
                .iter()
                .filter_map(|child| {
                    let walked = Self::walk(child, ref_counter, uid_counter, refs, true);
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
                .map(|child| Self::walk(child, ref_counter, uid_counter, refs, false))
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
    use crate::core::element_tree::{ElementData, NameSource};
    use crate::core::role::Role;

    fn make_tree() -> ElementNode {
        ElementNode::new(ElementData::new(Role::Window).with_name("Test Window", NameSource::Title))
            .with_children(vec![
                ElementNode::new(ElementData::new(Role::Group)).with_children(vec![
                    ElementNode::new(
                        ElementData::new(Role::Button).with_name("OK", NameSource::Title),
                    ),
                    ElementNode::new(
                        ElementData::new(Role::Button).with_name("Cancel", NameSource::Title),
                    ),
                ]),
                ElementNode::new(
                    ElementData::new(Role::TextField).with_name("Name", NameSource::Title),
                ),
            ])
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
                ElementNode::new(
                    ElementData::new(Role::StaticText).with_name("Label", NameSource::Title),
                ),
                ElementNode::new(ElementData::new(Role::Button).with_name("OK", NameSource::Title)),
            ]),
            ElementNode::new(
                ElementData::new(Role::StaticText).with_name("Footer", NameSource::Title),
            ),
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

    #[test]
    fn uid_assigned_to_all_elements() {
        let tree = make_tree();
        let assigner = RefAssigner::new();
        let result = assigner.assign(&tree, false);

        // Window gets uid=1
        assert_eq!(result.root.data.uid, Some(1));
        // Group gets uid=2
        assert_eq!(result.root.children[0].data.uid, Some(2));
        // OK button gets uid=3
        assert_eq!(result.root.children[0].children[0].data.uid, Some(3));
        // Cancel button gets uid=4
        assert_eq!(result.root.children[0].children[1].data.uid, Some(4));
        // TextField gets uid=5
        assert_eq!(result.root.children[1].data.uid, Some(5));
    }

    #[test]
    fn uid_is_depth_first() {
        let tree = ElementNode::new(ElementData::new(Role::Window)).with_children(vec![
            ElementNode::new(ElementData::new(Role::Group)).with_children(vec![
                ElementNode::new(ElementData::new(Role::Button).with_name("A", NameSource::Title)),
                ElementNode::new(ElementData::new(Role::Button).with_name("B", NameSource::Title)),
            ]),
            ElementNode::new(ElementData::new(Role::Group).with_name("Sidebar", NameSource::Title))
                .with_children(vec![ElementNode::new(
                    ElementData::new(Role::TextField).with_name("Search", NameSource::Title),
                )]),
        ]);

        let assigner = RefAssigner::new();
        let result = assigner.assign(&tree, false);

        // Depth-first order:
        // 1: Window
        // 2: Group (first)
        // 3: Button "A"
        // 4: Button "B"
        // 5: Group "Sidebar"
        // 6: TextField "Search"
        assert_eq!(result.root.data.uid, Some(1));
        assert_eq!(result.root.children[0].data.uid, Some(2));
        assert_eq!(result.root.children[0].children[0].data.uid, Some(3));
        assert_eq!(result.root.children[0].children[1].data.uid, Some(4));
        assert_eq!(result.root.children[1].data.uid, Some(5));
        assert_eq!(result.root.children[1].children[0].data.uid, Some(6));
    }

    #[test]
    fn uid_survives_interactive_only_pruning() {
        let tree = ElementNode::new(ElementData::new(Role::Window)).with_children(vec![
            ElementNode::new(ElementData::new(Role::Group)).with_children(vec![
                ElementNode::new(ElementData::new(Role::Button).with_name("OK", NameSource::Title)),
                ElementNode::new(
                    ElementData::new(Role::StaticText).with_name("Label", NameSource::Title),
                ),
            ]),
            ElementNode::new(
                ElementData::new(Role::TextField).with_name("Name", NameSource::Title),
            ),
        ]);

        let assigner = RefAssigner::new();
        let result = assigner.assign(&tree, true);

        // Pruning removes some nodes but the remaining ones keep their uids.
        // Window=1, Group=2, OK=3, Label=4 (pruned), TextField=5 (pruned)
        // After pruning: Window(1) + Group(2) + OK(3)
        assert_eq!(result.root.data.uid, Some(1));
        assert_eq!(result.root.children[0].data.uid, Some(2));
        assert_eq!(result.root.children[0].children[0].data.uid, Some(3));
        // Label was at uid=4 but pruned
        // TextField was at uid=5 but pruned
    }

    #[test]
    fn signature_assigned_to_all_elements() {
        let tree = make_tree();
        let assigner = RefAssigner::new();
        let result = assigner.assign(&tree, false);

        // Every element has a signature
        assert!(result.root.data.signature.is_some());
        assert!(result.root.children[0].data.signature.is_some());
        assert!(result.root.children[0].children[0].data.signature.is_some());
        assert!(result.root.children[0].children[1].data.signature.is_some());
        assert!(result.root.children[1].data.signature.is_some());
    }

    #[test]
    fn signature_deterministic_across_calls() {
        let tree = make_tree();
        let assigner = RefAssigner::new();

        let result_a = assigner.assign(&tree, false);
        let result_b = assigner.assign(&tree, false);

        // Same input → same signatures
        assert_eq!(result_a.root.data.signature, result_b.root.data.signature);
        assert_eq!(
            result_a.root.children[0].children[0].data.signature,
            result_b.root.children[0].children[0].data.signature
        );
    }

    #[test]
    fn different_signatures_for_different_content() {
        let tree_a =
            ElementNode::new(ElementData::new(Role::Window).with_name("Main", NameSource::Title));
        let tree_b =
            ElementNode::new(ElementData::new(Role::Window).with_name("Prefs", NameSource::Title));

        let assigner = RefAssigner::new();
        let result_a = assigner.assign(&tree_a, false);
        let result_b = assigner.assign(&tree_b, false);

        assert_ne!(result_a.root.data.signature, result_b.root.data.signature);
    }

    #[test]
    fn same_signature_for_undifferentiated_elements() {
        // Two trees with identical content get identical signatures
        let tree_a =
            ElementNode::new(ElementData::new(Role::Button).with_name("OK", NameSource::Title));
        let tree_b =
            ElementNode::new(ElementData::new(Role::Button).with_name("OK", NameSource::Title));

        let assigner = RefAssigner::new();
        let result_a = assigner.assign(&tree_a, false);
        let result_b = assigner.assign(&tree_b, false);

        assert_eq!(result_a.root.data.signature, result_b.root.data.signature);
    }
}
