//! Dependency graph construction and traversal.

use std::collections::{HashMap, HashSet};
use std::fmt;

use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use petgraph::Direction;

/// A node in the resolved dependency graph.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct ResolvedNode {
    pub group: String,
    pub artifact: String,
    pub version: String,
    pub scope: String,
}

impl ResolvedNode {
    /// `group:artifact` identifier (without version).
    pub fn key(&self) -> String {
        format!("{}:{}", self.group, self.artifact)
    }
}

impl fmt::Display for ResolvedNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.group, self.artifact, self.version)
    }
}

/// Edge label in the dependency graph.
#[derive(Debug, Clone)]
pub struct DepEdge {
    pub scope: String,
    pub optional: bool,
}

/// A resolved dependency graph backed by petgraph.
pub struct DependencyGraph {
    graph: DiGraph<ResolvedNode, DepEdge>,
    /// Lookup from `group:artifact` to node index (only the resolved version).
    index: HashMap<String, NodeIndex>,
    pub root: Option<NodeIndex>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            index: HashMap::new(),
            root: None,
        }
    }

    /// Add or retrieve a node. If the key already exists, returns the existing index.
    pub fn add_node(&mut self, node: ResolvedNode) -> NodeIndex {
        let key = node.key();
        if let Some(&idx) = self.index.get(&key) {
            return idx;
        }
        let idx = self.graph.add_node(node);
        self.index.insert(key, idx);
        idx
    }

    /// Set the root node of the graph (the project itself).
    pub fn set_root(&mut self, idx: NodeIndex) {
        self.root = Some(idx);
    }

    /// Add a dependency edge from `from` to `to`.
    pub fn add_edge(&mut self, from: NodeIndex, to: NodeIndex, edge: DepEdge) {
        if !self.graph.edges(from).any(|e| e.target() == to) {
            self.graph.add_edge(from, to, edge);
        }
    }

    /// Look up a node by `group:artifact`.
    pub fn find(&self, key: &str) -> Option<NodeIndex> {
        self.index.get(key).copied()
    }

    /// Get the node data for an index.
    pub fn node(&self, idx: NodeIndex) -> &ResolvedNode {
        &self.graph[idx]
    }

    /// All resolved nodes (excluding root).
    pub fn all_nodes(&self) -> Vec<&ResolvedNode> {
        self.graph
            .node_indices()
            .filter(|&idx| Some(idx) != self.root)
            .map(|idx| &self.graph[idx])
            .collect()
    }

    /// Direct dependencies of a node.
    pub fn dependencies_of(&self, idx: NodeIndex) -> Vec<(NodeIndex, &DepEdge)> {
        self.graph
            .edges_directed(idx, Direction::Outgoing)
            .map(|e| (e.target(), e.weight()))
            .collect()
    }

    /// Reverse dependencies (who depends on this node).
    pub fn dependents_of(&self, idx: NodeIndex) -> Vec<(NodeIndex, &DepEdge)> {
        self.graph
            .edges_directed(idx, Direction::Incoming)
            .map(|e| (e.source(), e.weight()))
            .collect()
    }

    /// Print the dependency tree to a string, grouping by scope.
    pub fn print_tree(&self, max_depth: Option<usize>) -> String {
        let mut output = String::new();
        let root = match self.root {
            Some(r) => r,
            None => return output,
        };

        let root_node = &self.graph[root];
        output.push_str(&format!("{}\n", root_node));

        let deps = self.dependencies_of(root);

        let mut compile_deps: Vec<(NodeIndex, &DepEdge)> = Vec::new();
        let mut test_deps: Vec<(NodeIndex, &DepEdge)> = Vec::new();
        let mut ksp_deps: Vec<(NodeIndex, &DepEdge)> = Vec::new();
        let mut kapt_deps: Vec<(NodeIndex, &DepEdge)> = Vec::new();

        for (idx, edge) in &deps {
            match edge.scope.as_str() {
                "test" => test_deps.push((*idx, edge)),
                "ksp" => ksp_deps.push((*idx, edge)),
                "kapt" => kapt_deps.push((*idx, edge)),
                _ => compile_deps.push((*idx, edge)),
            }
        }

        let has_non_compile =
            !test_deps.is_empty() || !ksp_deps.is_empty() || !kapt_deps.is_empty();
        let section_count = [&compile_deps, &test_deps, &ksp_deps, &kapt_deps]
            .iter()
            .filter(|s| !s.is_empty())
            .count();
        let show_headers = section_count > 1 || has_non_compile;
        let mut visited = HashSet::new();
        visited.insert(root);

        let mut sections_printed = 0usize;
        let total_sections = section_count;

        for (label, deps_list) in [
            ("[dependencies]", &compile_deps),
            ("[dev-dependencies]", &test_deps),
            ("[ksp]", &ksp_deps),
            ("[kapt]", &kapt_deps),
        ] {
            if deps_list.is_empty() {
                continue;
            }
            sections_printed += 1;
            if show_headers {
                output.push_str(&format!("{label}\n"));
            }
            let is_last_section = sections_printed == total_sections;
            let count = deps_list.len();
            for (i, (idx, _edge)) in deps_list.iter().enumerate() {
                let is_last = i == count - 1 && is_last_section;
                self.print_subtree(&mut output, *idx, "", is_last, 1, max_depth, &mut visited);
            }
        }

        output
    }

    #[allow(clippy::too_many_arguments)]
    fn print_subtree(
        &self,
        output: &mut String,
        idx: NodeIndex,
        prefix: &str,
        is_last: bool,
        depth: usize,
        max_depth: Option<usize>,
        visited: &mut HashSet<NodeIndex>,
    ) {
        let connector = if is_last { "└── " } else { "├── " };
        let node = &self.graph[idx];
        output.push_str(&format!("{prefix}{connector}{node}\n"));

        if let Some(max) = max_depth {
            if depth >= max {
                return;
            }
        }

        if !visited.insert(idx) {
            return;
        }

        let child_prefix = format!("{prefix}{}", if is_last { "    " } else { "│   " });
        let deps = self.dependencies_of(idx);
        let count = deps.len();
        for (i, (child, _)) in deps.iter().enumerate() {
            let is_last = i == count - 1;
            self.print_subtree(
                output,
                *child,
                &child_prefix,
                is_last,
                depth + 1,
                max_depth,
                visited,
            );
        }

        visited.remove(&idx);
    }

    /// Find the path from root to a specific dependency.
    ///
    /// Accepts either `group:artifact` or just `artifact` (partial match).
    pub fn find_path(&self, target_key: &str) -> Option<Vec<&ResolvedNode>> {
        let root = self.root?;
        let target = self.resolve_key(target_key)?;
        let mut path = Vec::new();
        let mut visited = HashSet::new();
        if self.dfs_path(root, target, &mut path, &mut visited) {
            Some(path.iter().map(|&idx| &self.graph[idx]).collect())
        } else {
            None
        }
    }

    /// Resolve a user-provided key to a node index.
    ///
    /// Tries exact `group:artifact` first, then falls back to matching by artifact name.
    fn resolve_key(&self, key: &str) -> Option<NodeIndex> {
        if let Some(&idx) = self.index.get(key) {
            return Some(idx);
        }
        // Partial match: find the first node whose artifact name matches
        for (full_key, &idx) in &self.index {
            let artifact = full_key.split(':').nth(1).unwrap_or("");
            if artifact == key {
                return Some(idx);
            }
        }
        None
    }

    fn dfs_path(
        &self,
        current: NodeIndex,
        target: NodeIndex,
        path: &mut Vec<NodeIndex>,
        visited: &mut HashSet<NodeIndex>,
    ) -> bool {
        path.push(current);
        if current == target {
            return true;
        }
        if !visited.insert(current) {
            path.pop();
            return false;
        }
        for edge in self.graph.edges(current) {
            if self.dfs_path(edge.target(), target, path, visited) {
                return true;
            }
        }
        path.pop();
        visited.remove(&current);
        false
    }

    /// Build an inverted dependency tree (reverse edges) for a single artifact.
    ///
    /// Accepts either `group:artifact` or just `artifact` (partial match).
    pub fn print_inverted_tree(&self, target_key: &str) -> String {
        let mut output = String::new();
        let Some(idx) = self.resolve_key(target_key) else {
            return output;
        };

        let node = &self.graph[idx];
        output.push_str(&format!("{node}\n"));

        let mut visited = HashSet::new();
        visited.insert(idx);

        let dependents = self.dependents_of(idx);
        let count = dependents.len();
        for (i, (dep_idx, _)) in dependents.iter().enumerate() {
            let is_last = i == count - 1;
            self.print_inverted_subtree(&mut output, *dep_idx, "", is_last, &mut visited);
        }

        output
    }

    fn print_inverted_subtree(
        &self,
        output: &mut String,
        idx: NodeIndex,
        prefix: &str,
        is_last: bool,
        visited: &mut HashSet<NodeIndex>,
    ) {
        let connector = if is_last { "└── " } else { "├── " };
        let node = &self.graph[idx];
        output.push_str(&format!("{prefix}{connector}{node}\n"));

        if !visited.insert(idx) {
            return;
        }

        let child_prefix = format!("{prefix}{}", if is_last { "    " } else { "│   " });
        let dependents = self.dependents_of(idx);
        let count = dependents.len();
        for (i, (dep_idx, _)) in dependents.iter().enumerate() {
            let is_last = i == count - 1;
            self.print_inverted_subtree(output, *dep_idx, &child_prefix, is_last, visited);
        }

        visited.remove(&idx);
    }

    /// Print a full inverted tree showing every node and what depends on it.
    pub fn print_full_inverted_tree(&self) -> String {
        let mut output = String::new();
        let root = match self.root {
            Some(r) => r,
            None => return output,
        };

        // Collect all leaf/non-root nodes and show their reverse chains
        let mut nodes: Vec<(NodeIndex, &ResolvedNode)> = self
            .graph
            .node_indices()
            .filter(|&idx| idx != root)
            .map(|idx| (idx, &self.graph[idx]))
            .collect();
        nodes.sort_by(|a, b| a.1.key().cmp(&b.1.key()));

        for (idx, node) in &nodes {
            let dependents = self.dependents_of(*idx);
            if dependents.is_empty() {
                continue;
            }
            output.push_str(&format!("{node}\n"));
            let count = dependents.len();
            for (i, (dep_idx, _)) in dependents.iter().enumerate() {
                let is_last = i == count - 1;
                let connector = if is_last { "└── " } else { "├── " };
                let dep_node = &self.graph[*dep_idx];
                output.push_str(&format!("{connector}{dep_node}\n"));
            }
            output.push('\n');
        }

        output
    }

    /// Number of nodes (excluding root).
    pub fn len(&self) -> usize {
        let total = self.graph.node_count();
        if self.root.is_some() {
            total.saturating_sub(1)
        } else {
            total
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(group: &str, artifact: &str, version: &str) -> ResolvedNode {
        ResolvedNode {
            group: group.to_string(),
            artifact: artifact.to_string(),
            version: version.to_string(),
            scope: "compile".to_string(),
        }
    }

    #[test]
    fn add_and_find() {
        let mut g = DependencyGraph::new();
        let node = make_node("org.example", "lib", "1.0");
        let idx = g.add_node(node.clone());
        assert_eq!(g.find("org.example:lib"), Some(idx));
        assert_eq!(g.node(idx).version, "1.0");
    }

    #[test]
    fn duplicate_add_returns_same_index() {
        let mut g = DependencyGraph::new();
        let idx1 = g.add_node(make_node("org.example", "lib", "1.0"));
        let idx2 = g.add_node(make_node("org.example", "lib", "1.0"));
        assert_eq!(idx1, idx2);
    }

    #[test]
    fn tree_printing() {
        let mut g = DependencyGraph::new();
        let root = g.add_node(make_node("com.example", "app", "1.0"));
        g.set_root(root);

        let a = g.add_node(make_node("org.a", "a", "1.0"));
        let b = g.add_node(make_node("org.b", "b", "2.0"));
        let c = g.add_node(make_node("org.c", "c", "3.0"));

        g.add_edge(
            root,
            a,
            DepEdge {
                scope: "compile".into(),
                optional: false,
            },
        );
        g.add_edge(
            root,
            b,
            DepEdge {
                scope: "compile".into(),
                optional: false,
            },
        );
        g.add_edge(
            a,
            c,
            DepEdge {
                scope: "compile".into(),
                optional: false,
            },
        );

        let tree = g.print_tree(None);
        assert!(tree.contains("com.example:app:1.0"));
        assert!(tree.contains("org.a:a:1.0"));
        assert!(tree.contains("org.b:b:2.0"));
        assert!(tree.contains("org.c:c:3.0"));
    }

    #[test]
    fn tree_groups_by_scope() {
        let mut g = DependencyGraph::new();
        let root = g.add_node(make_node("com.example", "app", "1.0"));
        g.set_root(root);

        let a = g.add_node(make_node("org.a", "lib", "1.0"));
        let b = g.add_node(ResolvedNode {
            group: "org.b".into(),
            artifact: "test-lib".into(),
            version: "2.0".into(),
            scope: "test".into(),
        });

        g.add_edge(
            root,
            a,
            DepEdge {
                scope: "compile".into(),
                optional: false,
            },
        );
        g.add_edge(
            root,
            b,
            DepEdge {
                scope: "test".into(),
                optional: false,
            },
        );

        let tree = g.print_tree(None);
        assert!(tree.contains("[dependencies]"));
        assert!(tree.contains("[dev-dependencies]"));
        let dep_pos = tree.find("[dependencies]").unwrap();
        let dev_pos = tree.find("[dev-dependencies]").unwrap();
        assert!(dep_pos < dev_pos);
    }

    #[test]
    fn find_path_exists() {
        let mut g = DependencyGraph::new();
        let root = g.add_node(make_node("com.example", "app", "1.0"));
        g.set_root(root);

        let a = g.add_node(make_node("org.a", "a", "1.0"));
        let b = g.add_node(make_node("org.b", "b", "1.0"));
        g.add_edge(
            root,
            a,
            DepEdge {
                scope: "compile".into(),
                optional: false,
            },
        );
        g.add_edge(
            a,
            b,
            DepEdge {
                scope: "compile".into(),
                optional: false,
            },
        );

        let path = g.find_path("org.b:b").unwrap();
        assert_eq!(path.len(), 3);
        assert_eq!(path[0].artifact, "app");
        assert_eq!(path[1].artifact, "a");
        assert_eq!(path[2].artifact, "b");
    }

    #[test]
    fn find_path_not_found() {
        let mut g = DependencyGraph::new();
        let root = g.add_node(make_node("com.example", "app", "1.0"));
        g.set_root(root);
        assert!(g.find_path("org.missing:lib").is_none());
    }

    #[test]
    fn inverted_tree() {
        let mut g = DependencyGraph::new();
        let root = g.add_node(make_node("com.example", "app", "1.0"));
        g.set_root(root);
        let a = g.add_node(make_node("org.a", "a", "1.0"));
        let b = g.add_node(make_node("org.b", "b", "1.0"));
        g.add_edge(
            root,
            a,
            DepEdge {
                scope: "compile".into(),
                optional: false,
            },
        );
        g.add_edge(
            a,
            b,
            DepEdge {
                scope: "compile".into(),
                optional: false,
            },
        );

        // Inverted tree for b should show a, then root
        let inv = g.print_inverted_tree("org.b:b");
        assert!(inv.contains("org.b:b:1.0"));
        assert!(inv.contains("org.a:a:1.0"));
        assert!(inv.contains("com.example:app:1.0"));
    }

    #[test]
    fn inverted_tree_partial_key() {
        let mut g = DependencyGraph::new();
        let root = g.add_node(make_node("com.example", "app", "1.0"));
        g.set_root(root);
        let a = g.add_node(make_node("org.a", "a", "1.0"));
        g.add_edge(
            root,
            a,
            DepEdge {
                scope: "compile".into(),
                optional: false,
            },
        );

        // Should resolve by artifact name alone
        let inv = g.print_inverted_tree("a");
        assert!(inv.contains("org.a:a:1.0"));
        assert!(inv.contains("com.example:app:1.0"));
    }

    #[test]
    fn find_path_partial_key() {
        let mut g = DependencyGraph::new();
        let root = g.add_node(make_node("com.example", "app", "1.0"));
        g.set_root(root);
        let a = g.add_node(make_node("org.a", "my-lib", "1.0"));
        g.add_edge(
            root,
            a,
            DepEdge {
                scope: "compile".into(),
                optional: false,
            },
        );

        // Should find path using just artifact name
        let path = g.find_path("my-lib").unwrap();
        assert_eq!(path.len(), 2);
        assert_eq!(path[1].artifact, "my-lib");
    }
}
