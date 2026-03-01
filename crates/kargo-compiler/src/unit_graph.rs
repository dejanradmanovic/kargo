//! Compilation unit dependency graph for build planning.
//!
//! Models the ordering constraints between compilation units:
//! KSP/KAPT code generation -> main compilation -> test compilation.
//! Uses `petgraph` to perform topological sorting.

use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::Topo;
use std::collections::HashMap;

use crate::unit::CompilationUnit;

/// A build graph that orders compilation units.
pub struct UnitGraph {
    graph: DiGraph<String, ()>,
    indices: HashMap<String, NodeIndex>,
    units: HashMap<String, CompilationUnit>,
}

impl UnitGraph {
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            indices: HashMap::new(),
            units: HashMap::new(),
        }
    }

    /// Add a compilation unit to the graph.
    pub fn add_unit(&mut self, unit: CompilationUnit) {
        let idx = self.graph.add_node(unit.name.clone());
        self.indices.insert(unit.name.clone(), idx);
        self.units.insert(unit.name.clone(), unit);
    }

    /// Declare that `dependent` depends on `dependency` (must be compiled after).
    pub fn add_dependency(&mut self, dependency: &str, dependent: &str) {
        if let (Some(&from), Some(&to)) =
            (self.indices.get(dependency), self.indices.get(dependent))
        {
            self.graph.add_edge(from, to, ());
        }
    }

    /// Return compilation units in topological order (dependencies first).
    pub fn topological_order(&self) -> Vec<&CompilationUnit> {
        let mut topo = Topo::new(&self.graph);
        let mut ordered = Vec::new();
        while let Some(idx) = topo.next(&self.graph) {
            let name = &self.graph[idx];
            if let Some(unit) = self.units.get(name) {
                ordered.push(unit);
            }
        }
        ordered
    }
}

impl Default for UnitGraph {
    fn default() -> Self {
        Self::new()
    }
}
