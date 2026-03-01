//! Resolution session cache for avoiding redundant POM lookups.
//!
//! The in-memory POM cache is handled directly within the resolver's BFS loop
//! via a `HashMap<String, Pom>`. This module provides any additional caching
//! utilities needed during a resolution session.
