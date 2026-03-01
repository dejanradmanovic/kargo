use std::collections::BTreeMap;
use std::path::Path;

/// Loads a `.kargo.env` file (shell-style `KEY=value` format).
///
/// `.kargo.env` holds build secrets and credentials (private registry auth,
/// signing passwords, CI tokens). Values are available via `${env:VAR}`
/// interpolation in `Kargo.toml` and as env vars during builds.
pub fn load_env_file(path: &Path) -> miette::Result<BTreeMap<String, String>> {
    let mut map = BTreeMap::new();
    if !path.is_file() {
        return Ok(map);
    }
    let content = std::fs::read_to_string(path).map_err(kargo_util::errors::KargoError::Io)?;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once('=') {
            map.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    Ok(map)
}

/// Interpolate `${env:VAR}` references in a string.
///
/// Looks up values first from the provided `env_overrides` map (populated
/// from `.kargo.env`), then falls back to actual process environment variables.
pub fn interpolate(input: &str, env_overrides: &BTreeMap<String, String>) -> String {
    let mut result = input.to_string();
    while let Some(start) = result.find("${env:") {
        let Some(end) = result[start..].find('}') else {
            break;
        };
        let end = start + end;
        let key = &result[start + 6..end];
        let value = env_overrides
            .get(key)
            .cloned()
            .or_else(|| std::env::var(key).ok())
            .unwrap_or_default();
        result.replace_range(start..=end, &value);
    }
    result
}
