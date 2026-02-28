//! Maven version parsing, comparison, and range matching.
//!
//! Maven versions use a custom ordering that differs from semver:
//! - Segments are split on `.` and `-`
//! - Numeric segments compare as numbers
//! - String qualifiers have a special ordering:
//!   `alpha` < `beta` < `milestone` < `rc` < `snapshot` < `""` (release) < `sp`
//! - SNAPSHOT versions sort before their release equivalent

use std::cmp::Ordering;
use std::fmt;

/// A parsed Maven version with comparable segments.
#[derive(Debug, Clone)]
pub struct MavenVersion {
    pub original: String,
    segments: Vec<Segment>,
}

impl PartialEq for MavenVersion {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for MavenVersion {}

#[derive(Debug, Clone, Eq, PartialEq)]
enum Segment {
    Numeric(u64),
    Qualifier(QualifierKind),
    Text(String),
}

/// Well-known Maven qualifiers with defined ordering.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
enum QualifierKind {
    Alpha,
    Beta,
    Milestone,
    Rc,
    Snapshot,
    Release,
    Sp,
}

impl MavenVersion {
    pub fn parse(version: &str) -> Self {
        let segments = parse_segments(version);
        Self {
            original: version.to_string(),
            segments,
        }
    }

    pub fn is_snapshot(&self) -> bool {
        self.original.ends_with("-SNAPSHOT")
    }

    /// The base version without the `-SNAPSHOT` suffix.
    pub fn base_version(&self) -> &str {
        self.original
            .strip_suffix("-SNAPSHOT")
            .unwrap_or(&self.original)
    }
}

impl fmt::Display for MavenVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.original)
    }
}

impl Ord for MavenVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        let max_len = self.segments.len().max(other.segments.len());
        for i in 0..max_len {
            let a = self.segments.get(i);
            let b = other.segments.get(i);
            let ord = compare_segments(a, b);
            if ord != Ordering::Equal {
                return ord;
            }
        }
        Ordering::Equal
    }
}

impl PartialOrd for MavenVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn compare_segments(a: Option<&Segment>, b: Option<&Segment>) -> Ordering {
    match (a, b) {
        (None, None) => Ordering::Equal,
        (Some(s), None) => compare_segment_to_empty(s),
        (None, Some(s)) => compare_segment_to_empty(s).reverse(),
        (Some(a), Some(b)) => compare_two_segments(a, b),
    }
}

fn compare_segment_to_empty(seg: &Segment) -> Ordering {
    match seg {
        Segment::Numeric(0) => Ordering::Equal,
        Segment::Numeric(n) => {
            if *n > 0 {
                Ordering::Greater
            } else {
                Ordering::Less
            }
        }
        Segment::Qualifier(q) => q.cmp(&QualifierKind::Release),
        Segment::Text(s) if s.is_empty() => Ordering::Equal,
        Segment::Text(_) => Ordering::Less,
    }
}

fn compare_two_segments(a: &Segment, b: &Segment) -> Ordering {
    match (a, b) {
        (Segment::Numeric(a), Segment::Numeric(b)) => a.cmp(b),
        (Segment::Qualifier(a), Segment::Qualifier(b)) => a.cmp(b),
        (Segment::Numeric(_), Segment::Qualifier(_)) => Ordering::Greater,
        (Segment::Qualifier(_), Segment::Numeric(_)) => Ordering::Less,
        (Segment::Numeric(_), Segment::Text(_)) => Ordering::Greater,
        (Segment::Text(_), Segment::Numeric(_)) => Ordering::Less,
        (Segment::Text(a), Segment::Text(b)) => a.to_lowercase().cmp(&b.to_lowercase()),
        (Segment::Qualifier(q), Segment::Text(_)) => {
            if *q >= QualifierKind::Release {
                Ordering::Greater
            } else {
                Ordering::Less
            }
        }
        (Segment::Text(_), Segment::Qualifier(q)) => {
            if *q >= QualifierKind::Release {
                Ordering::Less
            } else {
                Ordering::Greater
            }
        }
    }
}

fn parse_segments(version: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut current = String::new();

    for ch in version.chars() {
        if ch == '.' || ch == '-' {
            if !current.is_empty() {
                segments.push(classify(&current));
                current.clear();
            }
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        segments.push(classify(&current));
    }

    segments
}

fn classify(token: &str) -> Segment {
    if let Ok(n) = token.parse::<u64>() {
        return Segment::Numeric(n);
    }
    match token.to_lowercase().as_str() {
        "alpha" | "a" => Segment::Qualifier(QualifierKind::Alpha),
        "beta" | "b" => Segment::Qualifier(QualifierKind::Beta),
        "milestone" | "m" => Segment::Qualifier(QualifierKind::Milestone),
        "rc" | "cr" => Segment::Qualifier(QualifierKind::Rc),
        "snapshot" => Segment::Qualifier(QualifierKind::Snapshot),
        "" | "ga" | "final" | "release" => Segment::Qualifier(QualifierKind::Release),
        "sp" => Segment::Qualifier(QualifierKind::Sp),
        _ => Segment::Text(token.to_string()),
    }
}

/// A Maven version range expression.
///
/// Supports: `[1.0,2.0)`, `[1.0,]`, `(,2.0)`, `[1.0]` (exact).
#[derive(Debug, Clone)]
pub struct VersionRange {
    pub lower: Option<Bound>,
    pub upper: Option<Bound>,
}

#[derive(Debug, Clone)]
pub struct Bound {
    pub version: MavenVersion,
    pub inclusive: bool,
}

impl VersionRange {
    /// Parse a Maven version range string.
    ///
    /// Returns `None` for bare versions (not a range).
    pub fn parse(spec: &str) -> Option<Self> {
        let s = spec.trim();
        if !s.starts_with('[') && !s.starts_with('(') {
            return None;
        }

        let open_inclusive = s.starts_with('[');
        let close_inclusive = s.ends_with(']');
        let inner = &s[1..s.len() - 1];

        if let Some((lower, upper)) = inner.split_once(',') {
            let lower = lower.trim();
            let upper = upper.trim();
            Some(VersionRange {
                lower: if lower.is_empty() {
                    None
                } else {
                    Some(Bound {
                        version: MavenVersion::parse(lower),
                        inclusive: open_inclusive,
                    })
                },
                upper: if upper.is_empty() {
                    None
                } else {
                    Some(Bound {
                        version: MavenVersion::parse(upper),
                        inclusive: close_inclusive,
                    })
                },
            })
        } else {
            // Exact version: [1.0] means exactly 1.0
            let v = MavenVersion::parse(inner.trim());
            Some(VersionRange {
                lower: Some(Bound {
                    version: v.clone(),
                    inclusive: true,
                }),
                upper: Some(Bound {
                    version: v,
                    inclusive: true,
                }),
            })
        }
    }

    /// Check if a version satisfies this range.
    pub fn contains(&self, version: &MavenVersion) -> bool {
        if let Some(ref lower) = self.lower {
            let cmp = version.cmp(&lower.version);
            if lower.inclusive {
                if cmp == Ordering::Less {
                    return false;
                }
            } else if cmp != Ordering::Greater {
                return false;
            }
        }
        if let Some(ref upper) = self.upper {
            let cmp = version.cmp(&upper.version);
            if upper.inclusive {
                if cmp == Ordering::Greater {
                    return false;
                }
            } else if cmp != Ordering::Less {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_ordering() {
        let v1 = MavenVersion::parse("1.0");
        let v2 = MavenVersion::parse("2.0");
        assert!(v1 < v2);
    }

    #[test]
    fn three_part_ordering() {
        let v1 = MavenVersion::parse("1.0.0");
        let v2 = MavenVersion::parse("1.0.1");
        let v3 = MavenVersion::parse("1.1.0");
        assert!(v1 < v2);
        assert!(v2 < v3);
    }

    #[test]
    fn qualifier_ordering() {
        let alpha = MavenVersion::parse("1.0-alpha");
        let beta = MavenVersion::parse("1.0-beta");
        let rc = MavenVersion::parse("1.0-rc");
        let release = MavenVersion::parse("1.0");
        let sp = MavenVersion::parse("1.0-sp");

        assert!(alpha < beta);
        assert!(beta < rc);
        assert!(rc < release);
        assert!(release < sp);
    }

    #[test]
    fn snapshot_before_release() {
        let snap = MavenVersion::parse("1.0-SNAPSHOT");
        let rel = MavenVersion::parse("1.0");
        assert!(snap < rel);
    }

    #[test]
    fn trailing_zeros_equal() {
        let v1 = MavenVersion::parse("1.0");
        let v2 = MavenVersion::parse("1.0.0");
        assert_eq!(v1, v2);
    }

    #[test]
    fn numeric_vs_string() {
        let v1 = MavenVersion::parse("1.0.0");
        let v2 = MavenVersion::parse("1.0.0-jre");
        // Numeric 0 > text qualifier
        assert!(v1 > v2);
    }

    #[test]
    fn guava_style_versions() {
        let v1 = MavenVersion::parse("31.0-jre");
        let v2 = MavenVersion::parse("32.0-jre");
        assert!(v1 < v2);
    }

    #[test]
    fn is_snapshot() {
        let v = MavenVersion::parse("1.0-SNAPSHOT");
        assert!(v.is_snapshot());
        assert_eq!(v.base_version(), "1.0");

        let v2 = MavenVersion::parse("1.0.0");
        assert!(!v2.is_snapshot());
    }

    #[test]
    fn version_range_inclusive() {
        let range = VersionRange::parse("[1.0,2.0]").unwrap();
        assert!(range.contains(&MavenVersion::parse("1.0")));
        assert!(range.contains(&MavenVersion::parse("1.5")));
        assert!(range.contains(&MavenVersion::parse("2.0")));
        assert!(!range.contains(&MavenVersion::parse("0.9")));
        assert!(!range.contains(&MavenVersion::parse("2.1")));
    }

    #[test]
    fn version_range_exclusive_upper() {
        let range = VersionRange::parse("[1.0,2.0)").unwrap();
        assert!(range.contains(&MavenVersion::parse("1.0")));
        assert!(range.contains(&MavenVersion::parse("1.9.9")));
        assert!(!range.contains(&MavenVersion::parse("2.0")));
    }

    #[test]
    fn version_range_open_lower() {
        let range = VersionRange::parse("(,2.0)").unwrap();
        assert!(range.contains(&MavenVersion::parse("1.0")));
        assert!(!range.contains(&MavenVersion::parse("2.0")));
    }

    #[test]
    fn version_range_exact() {
        let range = VersionRange::parse("[1.5]").unwrap();
        assert!(range.contains(&MavenVersion::parse("1.5")));
        assert!(!range.contains(&MavenVersion::parse("1.4")));
        assert!(!range.contains(&MavenVersion::parse("1.6")));
    }

    #[test]
    fn bare_version_not_a_range() {
        assert!(VersionRange::parse("1.0").is_none());
    }

    #[test]
    fn display() {
        let v = MavenVersion::parse("1.8.0");
        assert_eq!(v.to_string(), "1.8.0");
    }
}
