//! POM file parsing: dependency declarations, parent inheritance, property interpolation, BOM imports.

use std::collections::BTreeMap;

use quick_xml::events::Event;
use quick_xml::Reader;

/// A parsed POM (Project Object Model) file.
#[derive(Debug, Clone, Default)]
pub struct Pom {
    pub group_id: Option<String>,
    pub artifact_id: Option<String>,
    pub version: Option<String>,
    pub packaging: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,

    pub parent: Option<ParentRef>,
    pub properties: BTreeMap<String, String>,
    pub dependencies: Vec<PomDependency>,
    pub dependency_management: Vec<PomDependency>,
    pub modules: Vec<String>,
    pub licenses: Vec<PomLicense>,
}

/// Reference to a parent POM.
#[derive(Debug, Clone)]
pub struct ParentRef {
    pub group_id: String,
    pub artifact_id: String,
    pub version: String,
    pub relative_path: Option<String>,
}

/// A dependency declared in a POM file.
#[derive(Debug, Clone)]
pub struct PomDependency {
    pub group_id: String,
    pub artifact_id: String,
    pub version: Option<String>,
    pub scope: Option<String>,
    pub optional: bool,
    pub classifier: Option<String>,
    pub type_: Option<String>,
    pub exclusions: Vec<PomExclusion>,
}

/// An exclusion within a dependency declaration.
#[derive(Debug, Clone)]
pub struct PomExclusion {
    pub group_id: String,
    pub artifact_id: Option<String>,
}

/// A license declared in a POM file.
#[derive(Debug, Clone)]
pub struct PomLicense {
    pub name: Option<String>,
    pub url: Option<String>,
}

impl Pom {
    /// Effective group ID (falls back to parent).
    pub fn effective_group_id(&self) -> Option<&str> {
        self.group_id
            .as_deref()
            .or(self.parent.as_ref().map(|p| p.group_id.as_str()))
    }

    /// Effective version (falls back to parent).
    pub fn effective_version(&self) -> Option<&str> {
        self.version
            .as_deref()
            .or(self.parent.as_ref().map(|p| p.version.as_str()))
    }

    /// Resolve `${property}` references in a string using POM properties
    /// and built-in project variables.
    pub fn interpolate(&self, input: &str) -> String {
        let mut result = input.to_string();
        let mut iterations = 0;
        while result.contains("${") && iterations < 20 {
            iterations += 1;
            let mut new = result.clone();
            while let Some(start) = new.find("${") {
                let Some(end) = new[start..].find('}') else {
                    break;
                };
                let key = &new[start + 2..start + end];
                let value = self.resolve_property(key);
                if let Some(val) = value {
                    new = format!("{}{}{}", &new[..start], val, &new[start + end + 1..]);
                } else {
                    break;
                }
            }
            if new == result {
                break;
            }
            result = new;
        }
        result
    }

    fn resolve_property(&self, key: &str) -> Option<String> {
        match key {
            "project.groupId" | "pom.groupId" => self.effective_group_id().map(|s| s.to_string()),
            "project.artifactId" | "pom.artifactId" => self.artifact_id.clone(),
            "project.version" | "pom.version" => self.effective_version().map(|s| s.to_string()),
            "project.packaging" | "pom.packaging" => self.packaging.clone(),
            "project.parent.groupId" => self.parent.as_ref().map(|p| p.group_id.clone()),
            "project.parent.version" => self.parent.as_ref().map(|p| p.version.clone()),
            _ => self.properties.get(key).cloned(),
        }
    }

    /// Interpolate all property references in dependencies and dependency management.
    pub fn resolve_properties(&mut self) {
        let pom_snapshot = self.clone();
        for dep in &mut self.dependencies {
            dep.group_id = pom_snapshot.interpolate(&dep.group_id);
            dep.artifact_id = pom_snapshot.interpolate(&dep.artifact_id);
            if let Some(ref v) = dep.version {
                dep.version = Some(pom_snapshot.interpolate(v));
            }
        }
        for dep in &mut self.dependency_management {
            dep.group_id = pom_snapshot.interpolate(&dep.group_id);
            dep.artifact_id = pom_snapshot.interpolate(&dep.artifact_id);
            if let Some(ref v) = dep.version {
                dep.version = Some(pom_snapshot.interpolate(v));
            }
        }
    }

    /// Merge a parent POM's properties and dependency management into this POM.
    pub fn apply_parent(&mut self, parent: &Pom) {
        for (k, v) in &parent.properties {
            self.properties
                .entry(k.clone())
                .or_insert_with(|| v.clone());
        }
        if self.group_id.is_none() {
            self.group_id = parent.effective_group_id().map(|s| s.to_string());
        }
        if self.version.is_none() {
            self.version = parent.effective_version().map(|s| s.to_string());
        }
        for dm in &parent.dependency_management {
            let dominated = self
                .dependency_management
                .iter()
                .any(|d| d.group_id == dm.group_id && d.artifact_id == dm.artifact_id);
            if !dominated {
                self.dependency_management.push(dm.clone());
            }
        }
    }

    /// Look up a version from dependency management for a given group:artifact.
    pub fn managed_version(&self, group_id: &str, artifact_id: &str) -> Option<&str> {
        self.dependency_management
            .iter()
            .find(|d| d.group_id == group_id && d.artifact_id == artifact_id)
            .and_then(|d| d.version.as_deref())
    }

    /// Return BOM imports from dependency management
    /// (entries with `scope = "import"` and `type = "pom"`).
    pub fn bom_imports(&self) -> Vec<&PomDependency> {
        self.dependency_management
            .iter()
            .filter(|d| {
                d.scope.as_deref() == Some("import") && d.type_.as_deref().unwrap_or("jar") == "pom"
            })
            .collect()
    }
}

/// Parse a POM XML string into a `Pom` struct.
pub fn parse_pom(xml: &str) -> miette::Result<Pom> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut pom = Pom::default();
    let mut path: Vec<String> = Vec::new();
    let mut text_buf = String::new();

    // Temporary accumulators for nested elements
    let mut current_dep: Option<PomDependency> = None;
    let mut current_exclusion: Option<PomExclusion> = None;
    let mut current_parent: Option<ParentRef> = None;
    let mut current_license: Option<PomLicense> = None;
    let mut in_dep_mgmt = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                path.push(tag.clone());
                text_buf.clear();

                let depth = path.len();
                let ctx = path_context(&path);

                match ctx.as_str() {
                    "project>dependencyManagement>dependencies>dependency"
                    | "project>dependencies>dependency" => {
                        if ctx.contains("dependencyManagement") {
                            in_dep_mgmt = true;
                        }
                        current_dep = Some(PomDependency {
                            group_id: String::new(),
                            artifact_id: String::new(),
                            version: None,
                            scope: None,
                            optional: false,
                            classifier: None,
                            type_: None,
                            exclusions: Vec::new(),
                        });
                    }
                    _ if ctx.ends_with(">exclusion") && current_dep.is_some() => {
                        current_exclusion = Some(PomExclusion {
                            group_id: String::new(),
                            artifact_id: None,
                        });
                    }
                    "project>parent" => {
                        current_parent = Some(ParentRef {
                            group_id: String::new(),
                            artifact_id: String::new(),
                            version: String::new(),
                            relative_path: None,
                        });
                    }
                    "project>licenses>license" => {
                        current_license = Some(PomLicense {
                            name: None,
                            url: None,
                        });
                    }
                    _ => {
                        // properties are children of <project><properties>
                        if depth == 3 && path.get(1).map(|s| s.as_str()) == Some("properties") {
                            // will capture text in End handler
                        }
                    }
                }
            }
            Ok(Event::Text(ref e)) => {
                text_buf = e.unescape().unwrap_or_default().to_string();
            }
            Ok(Event::End(ref e)) => {
                let _tag = String::from_utf8_lossy(e.name().as_ref()).to_string();
                let ctx = path_context(&path);
                let depth = path.len();

                // Properties: <project><properties><key>value</key></properties>
                if depth == 3 && path.get(1).map(|s| s.as_str()) == Some("properties") {
                    let prop_name = path.last().cloned().unwrap_or_default();
                    pom.properties.insert(prop_name, text_buf.clone());
                }

                // Handle dependency fields
                if let Some(ref mut dep) = current_dep {
                    if let Some(ref mut excl) = current_exclusion {
                        match path.last().map(|s| s.as_str()) {
                            Some("groupId") => excl.group_id = text_buf.clone(),
                            Some("artifactId") => excl.artifact_id = Some(text_buf.clone()),
                            _ => {}
                        }
                        if ctx.ends_with(">exclusion") {
                            if let Some(excl) = current_exclusion.take() {
                                dep.exclusions.push(excl);
                            }
                        }
                    } else {
                        match path.last().map(|s| s.as_str()) {
                            Some("groupId") if ctx.ends_with(">dependency>groupId") => {
                                dep.group_id = text_buf.clone();
                            }
                            Some("artifactId") if ctx.ends_with(">dependency>artifactId") => {
                                dep.artifact_id = text_buf.clone();
                            }
                            Some("version") if ctx.ends_with(">dependency>version") => {
                                dep.version = Some(text_buf.clone());
                            }
                            Some("scope") if ctx.ends_with(">dependency>scope") => {
                                dep.scope = Some(text_buf.clone());
                            }
                            Some("optional") if ctx.ends_with(">dependency>optional") => {
                                dep.optional = text_buf.trim() == "true";
                            }
                            Some("classifier") if ctx.ends_with(">dependency>classifier") => {
                                dep.classifier = Some(text_buf.clone());
                            }
                            Some("type") if ctx.ends_with(">dependency>type") => {
                                dep.type_ = Some(text_buf.clone());
                            }
                            _ => {}
                        }
                    }

                    if ctx == "project>dependencies>dependency"
                        || ctx == "project>dependencyManagement>dependencies>dependency"
                    {
                        if let Some(dep) = current_dep.take() {
                            if in_dep_mgmt {
                                pom.dependency_management.push(dep);
                            } else {
                                pom.dependencies.push(dep);
                            }
                        }
                        in_dep_mgmt = false;
                    }
                }

                // Handle parent fields
                if let Some(ref mut parent) = current_parent {
                    match path.last().map(|s| s.as_str()) {
                        Some("groupId") if ctx == "project>parent>groupId" => {
                            parent.group_id = text_buf.clone();
                        }
                        Some("artifactId") if ctx == "project>parent>artifactId" => {
                            parent.artifact_id = text_buf.clone();
                        }
                        Some("version") if ctx == "project>parent>version" => {
                            parent.version = text_buf.clone();
                        }
                        Some("relativePath") if ctx == "project>parent>relativePath" => {
                            parent.relative_path = Some(text_buf.clone());
                        }
                        _ => {}
                    }
                    if ctx == "project>parent" {
                        pom.parent = current_parent.take();
                    }
                }

                // Handle license fields
                if let Some(ref mut license) = current_license {
                    match path.last().map(|s| s.as_str()) {
                        Some("name") if ctx == "project>licenses>license>name" => {
                            license.name = Some(text_buf.clone());
                        }
                        Some("url") if ctx == "project>licenses>license>url" => {
                            license.url = Some(text_buf.clone());
                        }
                        _ => {}
                    }
                    if ctx == "project>licenses>license" {
                        if let Some(lic) = current_license.take() {
                            pom.licenses.push(lic);
                        }
                    }
                }

                // Top-level project fields
                if depth == 2 {
                    match path.last().map(|s| s.as_str()) {
                        Some("groupId") => pom.group_id = Some(text_buf.clone()),
                        Some("artifactId") => pom.artifact_id = Some(text_buf.clone()),
                        Some("version") => pom.version = Some(text_buf.clone()),
                        Some("packaging") => pom.packaging = Some(text_buf.clone()),
                        Some("name") => pom.name = Some(text_buf.clone()),
                        Some("description") => pom.description = Some(text_buf.clone()),
                        _ => {}
                    }
                }

                // Modules
                if ctx == "project>modules>module" {
                    pom.modules.push(text_buf.clone());
                }

                path.pop();
                text_buf.clear();
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(kargo_util::errors::KargoError::Generic {
                    message: format!("Failed to parse POM XML: {e}"),
                }
                .into());
            }
            _ => {}
        }
    }

    Ok(pom)
}

/// Build a context string from the current XML path for matching.
fn path_context(path: &[String]) -> String {
    path.join(">")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIMPLE_POM: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
    <modelVersion>4.0.0</modelVersion>
    <groupId>org.example</groupId>
    <artifactId>my-lib</artifactId>
    <version>1.0.0</version>
    <packaging>jar</packaging>

    <properties>
        <kotlin.version>2.3.0</kotlin.version>
    </properties>

    <dependencies>
        <dependency>
            <groupId>org.jetbrains.kotlin</groupId>
            <artifactId>kotlin-stdlib</artifactId>
            <version>${kotlin.version}</version>
        </dependency>
        <dependency>
            <groupId>junit</groupId>
            <artifactId>junit</artifactId>
            <version>4.13.2</version>
            <scope>test</scope>
        </dependency>
    </dependencies>
</project>"#;

    #[test]
    fn parse_simple_pom() {
        let pom = parse_pom(SIMPLE_POM).unwrap();
        assert_eq!(pom.group_id.as_deref(), Some("org.example"));
        assert_eq!(pom.artifact_id.as_deref(), Some("my-lib"));
        assert_eq!(pom.version.as_deref(), Some("1.0.0"));
        assert_eq!(pom.packaging.as_deref(), Some("jar"));
        assert_eq!(pom.dependencies.len(), 2);
        assert_eq!(pom.properties.get("kotlin.version").unwrap(), "2.3.0");
    }

    #[test]
    fn property_interpolation() {
        let mut pom = parse_pom(SIMPLE_POM).unwrap();
        pom.resolve_properties();
        assert_eq!(pom.dependencies[0].version.as_deref(), Some("2.3.0"));
    }

    #[test]
    fn test_scope_parsing() {
        let pom = parse_pom(SIMPLE_POM).unwrap();
        assert_eq!(pom.dependencies[0].scope, None);
        assert_eq!(pom.dependencies[1].scope.as_deref(), Some("test"));
    }

    #[test]
    fn dependency_management_and_bom() {
        let xml = r#"<?xml version="1.0"?>
<project>
    <groupId>org.example</groupId>
    <artifactId>parent</artifactId>
    <version>1.0.0</version>

    <dependencyManagement>
        <dependencies>
            <dependency>
                <groupId>org.jetbrains.kotlinx</groupId>
                <artifactId>kotlinx-coroutines-bom</artifactId>
                <version>1.8.0</version>
                <type>pom</type>
                <scope>import</scope>
            </dependency>
            <dependency>
                <groupId>com.google.guava</groupId>
                <artifactId>guava</artifactId>
                <version>32.0.0-jre</version>
            </dependency>
        </dependencies>
    </dependencyManagement>
</project>"#;
        let pom = parse_pom(xml).unwrap();
        assert_eq!(pom.dependency_management.len(), 2);
        let boms = pom.bom_imports();
        assert_eq!(boms.len(), 1);
        assert_eq!(boms[0].artifact_id, "kotlinx-coroutines-bom");

        assert_eq!(
            pom.managed_version("com.google.guava", "guava"),
            Some("32.0.0-jre")
        );
    }

    #[test]
    fn parent_ref_parsing() {
        let xml = r#"<?xml version="1.0"?>
<project>
    <parent>
        <groupId>org.example</groupId>
        <artifactId>parent-pom</artifactId>
        <version>2.0.0</version>
    </parent>
    <artifactId>child</artifactId>
</project>"#;
        let pom = parse_pom(xml).unwrap();
        assert!(pom.parent.is_some());
        assert_eq!(pom.effective_group_id(), Some("org.example"));
        assert_eq!(pom.effective_version(), Some("2.0.0"));
        let p = pom.parent.as_ref().unwrap();
        assert_eq!(p.group_id, "org.example");
        assert_eq!(p.version, "2.0.0");
    }

    #[test]
    fn exclusion_parsing() {
        let xml = r#"<?xml version="1.0"?>
<project>
    <groupId>org.example</groupId>
    <artifactId>app</artifactId>
    <version>1.0</version>
    <dependencies>
        <dependency>
            <groupId>com.example</groupId>
            <artifactId>lib</artifactId>
            <version>1.0</version>
            <exclusions>
                <exclusion>
                    <groupId>commons-logging</groupId>
                    <artifactId>commons-logging</artifactId>
                </exclusion>
            </exclusions>
        </dependency>
    </dependencies>
</project>"#;
        let pom = parse_pom(xml).unwrap();
        assert_eq!(pom.dependencies[0].exclusions.len(), 1);
        assert_eq!(
            pom.dependencies[0].exclusions[0].group_id,
            "commons-logging"
        );
    }

    #[test]
    fn license_parsing() {
        let xml = r#"<?xml version="1.0"?>
<project>
    <groupId>org.example</groupId>
    <artifactId>app</artifactId>
    <version>1.0</version>
    <licenses>
        <license>
            <name>Apache-2.0</name>
            <url>https://www.apache.org/licenses/LICENSE-2.0</url>
        </license>
    </licenses>
</project>"#;
        let pom = parse_pom(xml).unwrap();
        assert_eq!(pom.licenses.len(), 1);
        assert_eq!(pom.licenses[0].name.as_deref(), Some("Apache-2.0"));
    }

    #[test]
    fn project_version_interpolation() {
        let xml = r#"<?xml version="1.0"?>
<project>
    <groupId>org.example</groupId>
    <artifactId>lib</artifactId>
    <version>3.0.0</version>
    <dependencies>
        <dependency>
            <groupId>${project.groupId}</groupId>
            <artifactId>sibling</artifactId>
            <version>${project.version}</version>
        </dependency>
    </dependencies>
</project>"#;
        let mut pom = parse_pom(xml).unwrap();
        pom.resolve_properties();
        assert_eq!(pom.dependencies[0].group_id, "org.example");
        assert_eq!(pom.dependencies[0].version.as_deref(), Some("3.0.0"));
    }
}
