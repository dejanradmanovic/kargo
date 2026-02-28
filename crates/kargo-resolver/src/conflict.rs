//! Dependency conflict detection and resolution reporting.

use std::fmt;

/// A report of all version conflicts encountered during resolution.
#[derive(Debug, Default)]
pub struct ConflictReport {
    pub conflicts: Vec<VersionConflict>,
}

/// A single version conflict where multiple versions of the same artifact
/// were requested but only one was resolved.
#[derive(Debug, Clone)]
pub struct VersionConflict {
    pub group: String,
    pub artifact: String,
    pub requested: String,
    pub resolved: String,
    pub reason: String,
}

impl ConflictReport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, conflict: VersionConflict) {
        self.conflicts.push(conflict);
    }

    pub fn is_empty(&self) -> bool {
        self.conflicts.is_empty()
    }

    pub fn len(&self) -> usize {
        self.conflicts.len()
    }
}

impl fmt::Display for ConflictReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.conflicts.is_empty() {
            return write!(f, "No version conflicts.");
        }
        writeln!(f, "Version conflicts ({}):", self.conflicts.len())?;
        for c in &self.conflicts {
            writeln!(
                f,
                "  {}:{} requested {} but resolved {} ({})",
                c.group, c.artifact, c.requested, c.resolved, c.reason
            )?;
        }
        Ok(())
    }
}

impl fmt::Display for VersionConflict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}: {} -> {} ({})",
            self.group, self.artifact, self.requested, self.resolved, self.reason
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_report() {
        let report = ConflictReport::new();
        assert!(report.is_empty());
        assert_eq!(report.len(), 0);
        assert_eq!(report.to_string(), "No version conflicts.");
    }

    #[test]
    fn report_with_conflicts() {
        let mut report = ConflictReport::new();
        report.add(VersionConflict {
            group: "org.example".to_string(),
            artifact: "lib".to_string(),
            requested: "2.0".to_string(),
            resolved: "1.0".to_string(),
            reason: "nearest wins (depth 1 vs 2)".to_string(),
        });
        assert!(!report.is_empty());
        assert_eq!(report.len(), 1);
        let s = report.to_string();
        assert!(s.contains("org.example:lib"));
        assert!(s.contains("requested 2.0 but resolved 1.0"));
    }
}
