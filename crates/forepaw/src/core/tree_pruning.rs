/// Tree pruning: filter out menu bar and offscreen elements.
///
/// Used by both snapshot-time pruning (platform-specific) and
/// post-snapshot filtering (platform-agnostic).
use crate::core::element_tree::ElementNode;
use crate::core::role::Role;
use crate::core::types::Rect;

/// Options for pruning an element tree.
#[derive(Clone, Debug, Default)]
pub struct PruningOptions {
    /// Exclude menu bar elements (and their subtrees).
    pub exclude_menu_bar: bool,
    /// Exclude elements outside the viewport.
    pub exclude_offscreen: bool,
    /// Exclude zero-size elements (collapsed menus, hidden panels).
    pub skip_zero_size: bool,
}

/// Check if a node should be pruned (excluded from the tree).
///
/// Returns `true` if the node should be excluded.
///
/// Menu bar elements are excluded by role. Offscreen elements are excluded
/// by bounds overlap with the viewport. Zero-size elements are excluded
/// when `skip_zero_size` is true (only for elements with depth > 1).
#[must_use]
pub fn should_prune(
    node: &ElementNode,
    viewport: Option<&Rect>,
    depth: usize,
    options: &PruningOptions,
) -> bool {
    // Exclude menu bar elements.
    if options.exclude_menu_bar && node.data.role == Role::MenuBar {
        return true;
    }

    // Exclude zero-size subtrees (collapsed menus, hidden panels).
    if options.skip_zero_size {
        if let Some(bounds) = node.data.bounds {
            if bounds.width == 0.0 && bounds.height == 0.0 && depth > 1 {
                return true;
            }
        }
    }

    // Exclude offscreen elements (only elements with bounds and actual size).
    if options.exclude_offscreen {
        if let (Some(vp), Some(bounds)) = (viewport, node.data.bounds) {
            if bounds.width > 0.0 && bounds.height > 0.0 && !bounds_overlap(&bounds, vp) {
                return true;
            }
        }
    }

    false
}

/// Check if two rectangles overlap (including edge-touching).
fn bounds_overlap(a: &Rect, b: &Rect) -> bool {
    !(a.x + a.width <= b.x
        || b.x + b.width <= a.x
        || a.y + a.height <= b.y
        || b.y + b.height <= a.y)
}

/// Recursively filter a node and its subtree.
///
/// Returns `None` if the node should be excluded, `Some(filtered_node)` otherwise.
/// When a node is excluded, its entire subtree is pruned.
///
/// `depth` is the current depth in the tree (0 = root). Used for zero-size
/// pruning — zero-size elements at depth > 1 are excluded.
#[must_use]
pub fn prune_node(
    node: &ElementNode,
    viewport: Option<&Rect>,
    depth: usize,
    options: &PruningOptions,
) -> Option<ElementNode> {
    if should_prune(node, viewport, depth, options) {
        return None;
    }

    let children: Vec<ElementNode> = node
        .children
        .iter()
        .filter_map(|child| prune_node(child, viewport, depth + 1, options))
        .collect();

    Some(ElementNode {
        data: node.data.clone(),
        children,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::element_tree::ElementData;

    fn make_node(role: Role, x: f64, y: f64, w: f64, h: f64) -> ElementNode {
        ElementNode::new(ElementData::new(role).with_bounds(Rect::new(x, y, w, h)))
    }

    #[test]
    fn menu_bar_excluded() {
        let node = make_node(Role::MenuBar, 0.0, 0.0, 100.0, 20.0);
        let options = PruningOptions {
            exclude_menu_bar: true,
            ..Default::default()
        };
        assert!(should_prune(&node, None, 0, &options));
    }

    #[test]
    fn menu_bar_not_excluded_by_default() {
        let node = make_node(Role::MenuBar, 0.0, 0.0, 100.0, 20.0);
        let options = PruningOptions::default();
        assert!(!should_prune(&node, None, 0, &options));
    }

    #[test]
    fn offscreen_excluded() {
        let node = make_node(Role::Button, 500.0, 500.0, 100.0, 50.0);
        let viewport = Some(Rect::new(0.0, 0.0, 400.0, 300.0));
        let options = PruningOptions {
            exclude_offscreen: true,
            ..Default::default()
        };
        assert!(should_prune(&node, viewport.as_ref(), 0, &options));
    }

    #[test]
    fn onscreen_not_excluded() {
        let node = make_node(Role::Button, 100.0, 100.0, 100.0, 50.0);
        let viewport = Some(Rect::new(0.0, 0.0, 400.0, 300.0));
        let options = PruningOptions {
            exclude_offscreen: true,
            ..Default::default()
        };
        assert!(!should_prune(&node, viewport.as_ref(), 0, &options));
    }

    #[test]
    fn edge_touching_is_pruned() {
        // Node touches viewport edge — edge-touching counts as offscreen
        let node = make_node(Role::Button, 400.0, 100.0, 50.0, 50.0);
        let viewport = Some(Rect::new(0.0, 0.0, 400.0, 300.0));
        let options = PruningOptions {
            exclude_offscreen: true,
            ..Default::default()
        };
        assert!(should_prune(&node, viewport.as_ref(), 0, &options));
    }

    #[test]
    fn zero_size_not_pruned_by_offscreen() {
        // Zero-size elements should not be pruned (handled by skip_zero_size separately)
        let node = make_node(Role::Button, 500.0, 500.0, 0.0, 0.0);
        let viewport = Some(Rect::new(0.0, 0.0, 400.0, 300.0));
        let options = PruningOptions {
            exclude_offscreen: true,
            ..Default::default()
        };
        assert!(!should_prune(&node, viewport.as_ref(), 0, &options));
    }

    #[test]
    fn zero_size_pruned_at_depth_gt_1() {
        let node = make_node(Role::Button, 0.0, 0.0, 0.0, 0.0);
        let options = PruningOptions {
            skip_zero_size: true,
            ..Default::default()
        };
        // Depth 0: not pruned
        assert!(!should_prune(&node, None, 0, &options));
        // Depth 1: not pruned
        assert!(!should_prune(&node, None, 1, &options));
        // Depth 2: pruned
        assert!(should_prune(&node, None, 2, &options));
    }

    #[test]
    fn prune_node_removes_subtree() {
        let parent = ElementNode::new(ElementData::new(Role::Group))
            .with_children(vec![make_node(Role::MenuBar, 0.0, 0.0, 100.0, 20.0)]);
        let options = PruningOptions {
            exclude_menu_bar: true,
            ..Default::default()
        };
        let result = prune_node(&parent, None, 0, &options);
        assert!(result.is_some());
        let filtered = result.unwrap();
        assert!(filtered.children.is_empty());
    }
}
