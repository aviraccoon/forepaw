//! Generic ref→handle cache machinery shared by platform backends.
//!
//! Each backend walks its native accessibility tree once, building an
//! [`ElementNode`] tree AND a parallel [`HandleNode`] tree that retains the
//! native element handle for every node (so per-element resolve calls are
//! O(1) lookups instead of a full re-walk). [`flatten_handles`] then derives
//! a `ref_id → handle` map by numbering **interactive** nodes in the same
//! pre-order as [`RefAssigner`].
//!
//! The handle type `H` is backend-specific: Darwin stores a retained
//! `AXUIElementRef` (`Copy`, balanced by `CFRelease` on the map's `Drop`);
//! Windows stores an owned `IUIAutomationElement` (`Clone` = `AddRef`, RAII
//! `Drop`). The numbering logic and the parallel-tree contract are identical
//! across backends, so they live here with a single test suite rather than
//! as two copies that can silently drift apart.
//!
//! [`RefAssigner`]: crate::core::ref_assigner::RefAssigner

use std::collections::HashMap;
use std::hash::BuildHasher;

use crate::core::element_tree::ElementNode;

/// Mirror of the element tree that retains the native element handle for every
/// node. Built in lockstep with [`ElementNode`] (same recursion, same pruning
/// early-returns) so its shape is identical: every code path in a backend's
/// `build_tree` must produce a matching `HandleNode`, and the stale-children
/// retry must update both slots together. [`flatten_handles`] relies on this,
/// zipping `element.children` with `handle.children`.
#[derive(Debug, Clone)]
#[must_use]
pub struct HandleNode<H> {
    /// Retained native handle for this node, if any.
    pub handle: Option<H>,
    /// Parallel children, in the same order as the matching `ElementNode`.
    pub children: Vec<Self>,
}

impl<H> Default for HandleNode<H> {
    /// A node with no handle and no children (used by depth-limit / fetch-fail
    /// early returns in a backend's `build_tree`). Unconditional so backends
    /// whose handle type isn't `Default` can still construct it.
    fn default() -> Self {
        Self {
            handle: None,
            children: Vec::new(),
        }
    }
}

impl<H> HandleNode<H> {
    /// Construct a leaf node carrying `handle` and no children.
    pub fn leaf(handle: H) -> Self {
        Self {
            handle: Some(handle),
            children: Vec::new(),
        }
    }
}

/// Flatten the handle tree into a ref→handle map, mirroring `RefAssigner`'s
/// pre-order counter over **interactive** nodes.
///
/// `H: Clone` lets owned-handle backends (Windows COM) take an `AddRef` per
/// insertion; `Copy` backends (Darwin) clone cheaply. The [`ElementNode`] is
/// the single source of interactivity (its role); the `HandleNode` just
/// carries the retained handle. `handle` is `Some` for every real node in a
/// correctly built tree, so the `if let Some` only guards against a structural
/// mismatch.
pub fn flatten_handles<H: Clone, S: BuildHasher>(
    element: &ElementNode,
    handle: &HandleNode<H>,
    counter: &mut i32,
    map: &mut HashMap<i32, H, S>,
) {
    if element.is_interactive() {
        if let Some(h) = &handle.handle {
            map.insert(*counter, h.clone());
        }
        *counter += 1;
    }
    for (child_elem, child_handle) in element.children.iter().zip(&handle.children) {
        flatten_handles(child_elem, child_handle, counter, map);
    }
}

/// Build a ref→handle map from the parallel trees, numbering interactive nodes
/// in pre-order starting at 1 (matching `RefAssigner`).
pub fn build_ref_handle_map<H: Clone, S: BuildHasher + Default>(
    element_root: &ElementNode,
    handle_root: &HandleNode<H>,
) -> HashMap<i32, H, S> {
    let mut map: HashMap<i32, H, S> = HashMap::default();
    let mut counter: i32 = 1;
    flatten_handles(element_root, handle_root, &mut counter, &mut map);
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::element_tree::ElementData;
    use crate::core::ref_assigner::RefAssigner;
    use crate::core::role::Role;

    #[test]
    fn flatten_handles_mirrors_ref_assigner_order() {
        // Window -> [ Group -> [Btn(1), Btn(2)], TextField(3) ].
        // Handles are retained for every node (as a real `build_tree` does);
        // only the interactive Button/TextField nodes consume ref numbers,
        // matching `RefAssigner`.
        let element_tree = ElementNode::new(ElementData::new(Role::Window)).with_children(vec![
            ElementNode::new(ElementData::new(Role::Group)).with_children(vec![
                ElementNode::new(ElementData::new(Role::Button)),
                ElementNode::new(ElementData::new(Role::Button)),
            ]),
            ElementNode::new(ElementData::new(Role::TextField)),
        ]);
        let handle_tree = HandleNode {
            handle: Some(0_usize),
            children: vec![
                HandleNode {
                    handle: Some(0_usize),
                    children: vec![HandleNode::leaf(1), HandleNode::leaf(2)],
                },
                HandleNode::leaf(3),
            ],
        };
        let mut map = HashMap::new();
        let mut counter = 1;
        flatten_handles(&element_tree, &handle_tree, &mut counter, &mut map);
        assert_eq!(map.len(), 3);
        assert_eq!(*map.get(&1).expect("ref 1"), 1);
        assert_eq!(*map.get(&2).expect("ref 2"), 2);
        assert_eq!(*map.get(&3).expect("ref 3"), 3);
        assert_eq!(counter, 4);
    }

    #[test]
    fn flatten_handles_assigns_ref_to_pruned_interactive_leaf() {
        // A pruned interactive leaf still gets a ref (it remains in the tree);
        // a non-interactive parent does not.
        let element_tree = ElementNode::new(ElementData::new(Role::Window))
            .with_children(vec![ElementNode::new(ElementData::new(Role::Button))]);
        let tree = HandleNode {
            handle: Some(0_usize),                // window, non-interactive -> no ref
            children: vec![HandleNode::leaf(10)], // interactive leaf (e.g. offscreen button)
        };
        let mut map = HashMap::new();
        let mut counter = 1;
        flatten_handles(&element_tree, &tree, &mut counter, &mut map);
        assert_eq!(map.len(), 1);
        assert_eq!(*map.get(&1).expect("ref 1"), 10);
    }

    // --- Lockstep invariant: flatten_handles numbering == RefAssigner numbering ---

    /// Generate a random `ElementNode` tree and the parallel `HandleNode<usize>`
    /// tree built by the same rule a real `build_tree` uses (`handle = Some` for
    /// every node; interactivity lives on the `ElementNode`). Each node carries
    /// a unique sentinel so ordering can be checked. Deterministic LCG -- no
    /// `proptest` dependency.
    fn gen_tree(seed: u32) -> (ElementNode, HandleNode<usize>) {
        fn gen(
            depth: usize,
            next: &mut impl FnMut() -> u32,
            node_id: &mut usize,
        ) -> (ElementNode, HandleNode<usize>) {
            // ~half interactive, half structural.
            let role = if next().is_multiple_of(2) {
                match next() % 3 {
                    0 => Role::Button,
                    1 => Role::TextField,
                    _ => Role::CheckBox,
                }
            } else {
                match next() % 3 {
                    0 => Role::Window,
                    1 => Role::Group,
                    _ => Role::StaticText,
                }
            };
            *node_id += 1;
            let sentinel = *node_id;

            let child_count = if depth < 4 {
                usize::try_from(next() % 4).unwrap()
            } else {
                0
            };
            let mut children = Vec::with_capacity(child_count);
            let mut child_handles = Vec::with_capacity(child_count);
            for _ in 0..child_count {
                let (c, h) = gen(depth + 1, next, node_id);
                children.push(c);
                child_handles.push(h);
            }
            (
                ElementNode::new(ElementData::new(role)).with_children(children),
                HandleNode {
                    handle: Some(sentinel),
                    children: child_handles,
                },
            )
        }

        let mut state = seed;
        let mut next = || {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            state
        };
        gen(0, &mut next, &mut 0)
    }

    /// Walk `RefAssigner`'s annotated tree in lockstep with the `HandleNode`
    /// tree (identical structure under `interactive_only = false`) and pair each
    /// ref it assigned with the corresponding node's sentinel handle.
    fn assigned_sentinels(
        assigned_root: &ElementNode,
        handles: &HandleNode<usize>,
    ) -> HashMap<i32, usize> {
        fn walk(node: &ElementNode, h: &HandleNode<usize>, map: &mut HashMap<i32, usize>) {
            if let Some(r) = node.data.reference {
                let sentinel = h.handle.expect("interactive node has a handle");
                map.insert(r.id, sentinel);
            }
            for (c, ch) in node.children.iter().zip(&h.children) {
                walk(c, ch, map);
            }
        }
        let mut map = HashMap::new();
        walk(assigned_root, handles, &mut map);
        map
    }

    #[test]
    fn flatten_handles_numbering_matches_ref_assigner_across_shapes() {
        // The whole ref→handle cache rests on this: `flatten_handles` must
        // number interactive nodes in exactly the same pre-order as
        // `RefAssigner`, across arbitrary tree shapes. This pins the numbering
        // agreement between the two independently-implemented walks.
        for seed in 1..=50_u32 {
            let (tree, handles) = gen_tree(seed);
            let assigned = RefAssigner::new().assign(&tree, false);
            let expected = assigned_sentinels(&assigned.root, &handles);

            let mut actual: HashMap<i32, usize> = HashMap::new();
            let mut counter: i32 = 1;
            flatten_handles(&tree, &handles, &mut counter, &mut actual);
            assert_eq!(actual.len(), expected.len(), "seed {seed}: map size");
            for (id, sentinel) in &expected {
                let handle = actual
                    .get(id)
                    .unwrap_or_else(|| panic!("seed {seed}: ref {id} missing"));
                assert_eq!(
                    *handle, *sentinel,
                    "seed {seed}: ref {id} points at the wrong node"
                );
            }
        }
    }
}
