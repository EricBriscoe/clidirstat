use std::path::PathBuf;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u32);

impl NodeId {
    pub const ROOT: NodeId = NodeId(0);
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum NodeKind {
    Dir,
    File,
    Symlink,
    Other,
}

impl NodeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            NodeKind::Dir => "dir",
            NodeKind::File => "file",
            NodeKind::Symlink => "symlink",
            NodeKind::Other => "other",
        }
    }
}

#[derive(Debug)]
pub struct Node {
    pub name: String,
    pub parent: Option<NodeId>,
    pub kind: NodeKind,
    pub children: Vec<NodeId>,
    /// Allocated bytes on disk (`st_blocks * 512`). The only size metric the
    /// tool tracks; see CLAUDE.md "Invariants".
    pub size: u64,
    pub had_error: bool,
}

#[derive(Debug)]
pub struct Tree {
    nodes: Vec<Node>,
    pub root_path: PathBuf,
    pub skipped_count: u64,
    pub error_count: u64,
    /// Bumped on every mutation. UI uses this to detect when its cached
    /// treemap layout is stale.
    pub generation: u64,
}

impl Tree {
    pub fn new(root_path: PathBuf, root_name: String) -> Self {
        let root = Node {
            name: root_name,
            parent: None,
            kind: NodeKind::Dir,
            children: Vec::new(),
            size: 0,
            had_error: false,
        };
        Self {
            nodes: vec![root],
            root_path,
            skipped_count: 0,
            error_count: 0,
            generation: 0,
        }
    }

    pub fn push(&mut self, parent: NodeId, node: Node) -> NodeId {
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(node);
        self.nodes[parent.index()].children.push(id);
        self.generation = self.generation.wrapping_add(1);
        id
    }

    /// Walk parent links from `id` upward, saturating-adding `size` to each
    /// ancestor. Used by the streaming scanner so the tree's directory
    /// totals are always current without a final post-order pass. Bumps
    /// the generation counter.
    pub fn bump_ancestors(&mut self, id: NodeId, size: u64) {
        if size == 0 {
            return;
        }
        let mut cur = self.nodes[id.index()].parent;
        while let Some(p) = cur {
            let n = &mut self.nodes[p.index()];
            n.size = n.size.saturating_add(size);
            cur = n.parent;
        }
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn get(&self, id: NodeId) -> &Node {
        &self.nodes[id.index()]
    }

    pub fn get_mut(&mut self, id: NodeId) -> &mut Node {
        &mut self.nodes[id.index()]
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    pub fn path_of(&self, mut id: NodeId) -> PathBuf {
        let mut parts: Vec<&str> = Vec::new();
        while let Some(parent) = self.nodes[id.index()].parent {
            parts.push(&self.nodes[id.index()].name);
            id = parent;
        }
        let mut path = self.root_path.clone();
        for part in parts.iter().rev() {
            path.push(part);
        }
        path
    }

    /// Bottom-up size aggregation. Recomputes directory totals from children.
    pub fn aggregate(&mut self) {
        let order = self.post_order(NodeId::ROOT);
        for id in order {
            let node = &self.nodes[id.index()];
            if matches!(node.kind, NodeKind::Dir) {
                let mut sum = 0u64;
                for &child in &node.children.clone() {
                    sum = sum.saturating_add(self.nodes[child.index()].size);
                }
                let n = &mut self.nodes[id.index()];
                n.size = n.size.saturating_add(sum);
            }
        }
    }

    fn post_order(&self, root: NodeId) -> Vec<NodeId> {
        let mut order = Vec::with_capacity(self.nodes.len());
        let mut stack: Vec<(NodeId, usize)> = vec![(root, 0)];
        while let Some((id, idx)) = stack.last().copied() {
            let node = &self.nodes[id.index()];
            if idx < node.children.len() {
                stack.last_mut().unwrap().1 = idx + 1;
                stack.push((node.children[idx], 0));
            } else {
                order.push(id);
                stack.pop();
            }
        }
        order
    }

    /// Children sorted descending by size.
    pub fn sorted_children(&self, id: NodeId) -> Vec<NodeId> {
        let mut v = self.nodes[id.index()].children.clone();
        v.sort_by_key(|c| std::cmp::Reverse(self.nodes[c.index()].size));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, parent: NodeId, size: u64) -> Node {
        Node {
            name: name.into(),
            parent: Some(parent),
            kind: NodeKind::File,
            children: vec![],
            size,
            had_error: false,
        }
    }

    fn dir(name: &str, parent: NodeId) -> Node {
        Node {
            name: name.into(),
            parent: Some(parent),
            kind: NodeKind::Dir,
            children: vec![],
            size: 0,
            had_error: false,
        }
    }

    #[test]
    fn bump_ancestors_propagates_through_chain() {
        let mut t = Tree::new(PathBuf::from("/x"), "/x".into());
        let a = t.push(NodeId::ROOT, dir("a", NodeId::ROOT));
        let f = t.push(a, file("f", a, 4096));
        t.bump_ancestors(f, 4096);
        assert_eq!(t.get(a).size, 4096);
        assert_eq!(t.get(NodeId::ROOT).size, 4096);
    }

    #[test]
    fn generation_advances_on_mutations() {
        let mut t = Tree::new(PathBuf::from("/x"), "/x".into());
        let g0 = t.generation;
        let id = t.push(NodeId::ROOT, file("y", NodeId::ROOT, 1));
        assert!(t.generation > g0);
        let g1 = t.generation;
        t.bump_ancestors(id, 1);
        assert!(t.generation > g1);
    }

    #[test]
    fn aggregate_sums_children() {
        let mut t = Tree::new(PathBuf::from("/x"), "/x".into());
        let a = t.push(NodeId::ROOT, dir("a", NodeId::ROOT));
        t.push(a, file("f1", a, 4096));
        t.push(a, file("f2", a, 4096));
        t.aggregate();
        assert_eq!(t.get(a).size, 8192);
        assert_eq!(t.get(NodeId::ROOT).size, 8192);
    }
}
