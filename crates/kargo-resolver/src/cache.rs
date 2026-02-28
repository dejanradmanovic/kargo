//! Resolution session cache for avoiding redundant POM lookups.
//!
//! The in-memory POM cache is handled directly within the resolver's BFS loop
//! via a `HashMap<String, Pom>`. This module provides any additional caching
//! utilities needed during a resolution session.

use std::collections::HashSet;

/// Tracks which coordinates have been visited during resolution
/// to prevent infinite loops in circular dependency chains.
#[derive(Debug, Default)]
pub struct VisitedSet {
    visited: HashSet<String>,
}

impl VisitedSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark a coordinate as visited. Returns `false` if already visited.
    pub fn visit(&mut self, group: &str, artifact: &str, version: &str) -> bool {
        self.visited.insert(format!("{group}:{artifact}:{version}"))
    }

    pub fn contains(&self, group: &str, artifact: &str, version: &str) -> bool {
        self.visited
            .contains(&format!("{group}:{artifact}:{version}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visited_tracking() {
        let mut set = VisitedSet::new();
        assert!(set.visit("org.example", "lib", "1.0"));
        assert!(!set.visit("org.example", "lib", "1.0"));
        assert!(set.contains("org.example", "lib", "1.0"));
        assert!(!set.contains("org.example", "lib", "2.0"));
    }
}
