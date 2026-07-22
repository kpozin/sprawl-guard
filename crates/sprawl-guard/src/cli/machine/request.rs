use std::path::{Component, Path, PathBuf};

use serde::de::Error as _;
use serde::{Deserialize, Deserializer};

use super::contract::{MACHINE_PROTOCOL_VERSION, MachineHostPathStyle};

/// JSON request consumed by `sprawl-guard machine run`.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MachineRunRequest {
    protocol_version: MachineProtocolVersion,
    workspace_root: MachineWorkspaceRoot,
    host_path_mapping: MachineHostPathMapping,
    argv: MachineArgv,
}

impl MachineRunRequest {
    /// Absolute path that the Rust process should treat as the machine workspace root.
    pub(super) fn workspace_root(&self) -> &Path {
        &self.workspace_root.0
    }

    /// Normal CLI argv excluding the binary name.
    pub(super) fn argv(&self) -> &[String] {
        &self.argv.0
    }

    /// Rewrites one parsed CLI path into the machine workspace.
    pub(super) fn remap_path(&self, path: &Path, effective_root: &Path) -> Result<PathBuf, String> {
        let raw_path = path.to_string_lossy();
        if path.is_absolute() {
            let candidate = lexical_normalize(path)?;
            if path.starts_with(self.workspace_root()) {
                require_under_workspace(&candidate, self.workspace_root())?;
                return Ok(candidate);
            }
            if is_under_workspace(&candidate, self.workspace_root())? {
                return Ok(candidate);
            }
        }

        if let Some(mapped) = self
            .host_path_mapping
            .remap_host_absolute(&raw_path, self.workspace_root())?
        {
            return Ok(mapped);
        }

        let candidate = if path.is_absolute() {
            lexical_normalize(path)?
        } else {
            lexical_normalize(&effective_root.join(path))?
        };
        require_under_workspace(&candidate, self.workspace_root())?;
        Ok(candidate)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MachineArgv(Vec<String>);

impl<'de> Deserialize<'de> for MachineArgv {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let argv = Vec::<String>::deserialize(deserializer)?;
        if argv
            .first()
            .is_some_and(|argument| argument == crate::CLI_NAME)
        {
            return Err(D::Error::custom(
                "argv must exclude the sprawl-guard binary name",
            ));
        }
        Ok(Self(argv))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MachineProtocolVersion;

impl<'de> Deserialize<'de> for MachineProtocolVersion {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let version = u8::deserialize(deserializer)?;
        if version != MACHINE_PROTOCOL_VERSION {
            return Err(D::Error::custom(format!(
                "unsupported protocol_version {version}; expected {MACHINE_PROTOCOL_VERSION}"
            )));
        }
        Ok(Self)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MachineWorkspaceRoot(PathBuf);

impl<'de> Deserialize<'de> for MachineWorkspaceRoot {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        let path = lexical_normalize(Path::new(&value)).map_err(D::Error::custom)?;
        if !path.is_absolute() {
            return Err(D::Error::custom(format!(
                "workspace_root must be absolute: {value}"
            )));
        }
        Ok(Self(path))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum MachineHostPathMapping {
    None,
    Alias(MachineHostRoot),
}

impl MachineHostPathMapping {
    fn remap_host_absolute(
        &self,
        raw_path: &str,
        workspace_root: &Path,
    ) -> Result<Option<PathBuf>, String> {
        match self {
            Self::None => Ok(None),
            Self::Alias(host_root) => host_root.remap_absolute(raw_path, workspace_root),
        }
    }
}

impl<'de> Deserialize<'de> for MachineHostPathMapping {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawMachineHostPathMapping::deserialize(deserializer)?;
        match raw {
            RawMachineHostPathMapping::None => Ok(Self::None),
            RawMachineHostPathMapping::Alias { host_root, style } => {
                MachineHostRoot::parse(&host_root, style)
                    .map(Self::Alias)
                    .map_err(D::Error::custom)
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum RawMachineHostPathMapping {
    None,
    Alias {
        host_root: String,
        style: MachineHostPathStyle,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum MachineHostRoot {
    Posix(Vec<String>),
    Windows(WindowsAbsolutePath),
}

impl MachineHostRoot {
    fn parse(host_root: &str, style: MachineHostPathStyle) -> Result<Self, String> {
        match style {
            MachineHostPathStyle::Posix => {
                normalize_posix_absolute(host_root, "host_path_mapping.host_root").map(Self::Posix)
            }
            MachineHostPathStyle::Windows => WindowsAbsolutePath::parse(host_root)
                .map(Self::Windows)
                .ok_or_else(|| {
                    format!(
                        "host_path_mapping.host_root must be an absolute Windows path: {host_root}"
                    )
                }),
        }
    }

    fn remap_absolute(
        &self,
        raw_path: &str,
        workspace_root: &Path,
    ) -> Result<Option<PathBuf>, String> {
        match self {
            Self::Posix(host_root) => {
                remap_posix_host_absolute(host_root, raw_path, workspace_root)
            }
            Self::Windows(host_root) => {
                remap_windows_host_absolute(host_root, raw_path, workspace_root)
            }
        }
    }
}

fn remap_posix_host_absolute(
    host_root: &[String],
    raw_path: &str,
    workspace_root: &Path,
) -> Result<Option<PathBuf>, String> {
    if !raw_path.starts_with('/') {
        return Ok(None);
    }
    let candidate = normalize_posix_absolute(raw_path, "argv path")?;
    let suffix = strip_component_prefix(&candidate, host_root).ok_or_else(|| {
        format!("host absolute path is outside host_path_mapping.host_root: {raw_path}")
    })?;
    Ok(Some(workspace_root.join(path_buf_from_components(suffix))))
}

fn normalize_posix_absolute(value: &str, field: &str) -> Result<Vec<String>, String> {
    if !value.starts_with('/') {
        return Err(format!("{field} must be an absolute POSIX path: {value}"));
    }
    let mut components = Vec::new();
    for component in value.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                components.pop();
            }
            component => components.push(component.to_owned()),
        }
    }
    Ok(components)
}

fn remap_windows_host_absolute(
    host_root: &WindowsAbsolutePath,
    raw_path: &str,
    workspace_root: &Path,
) -> Result<Option<PathBuf>, String> {
    if has_unsupported_windows_absolute_prefix(raw_path) {
        return Err(format!("unsupported Windows path prefix: {raw_path}"));
    }
    let Some(candidate) = WindowsAbsolutePath::parse(raw_path) else {
        return Ok(None);
    };
    if !candidate.has_prefix(host_root) {
        return Err(format!(
            "host absolute path is outside host_path_mapping.host_root: {raw_path}"
        ));
    }
    Ok(Some(workspace_root.join(path_buf_from_components(
        &candidate.components[host_root.components.len()..],
    ))))
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WindowsAbsolutePath {
    anchor: String,
    components: Vec<String>,
}

impl WindowsAbsolutePath {
    fn parse(value: &str) -> Option<Self> {
        let normalized = value.replace('/', "\\");
        if has_unsupported_windows_absolute_prefix(&normalized) {
            return None;
        }
        if let Some(path) = Self::parse_unc(&normalized) {
            return Some(path);
        }
        Self::parse_drive(&normalized)
    }

    fn parse_drive(value: &str) -> Option<Self> {
        let mut chars = value.chars();
        let drive = chars.next()?;
        if !drive.is_ascii_alphabetic() || chars.next()? != ':' || chars.next()? != '\\' {
            return None;
        }
        let remainder = &value[3..];
        Some(Self {
            anchor: format!("drive:{}", drive.to_ascii_lowercase()),
            components: normalize_windows_components(remainder),
        })
    }

    fn parse_unc(value: &str) -> Option<Self> {
        let remainder = value.strip_prefix("\\\\")?;
        let mut parts = remainder.split('\\');
        let server = parts.next()?.to_ascii_lowercase();
        let share = parts.next()?.to_ascii_lowercase();
        if server.is_empty() || share.is_empty() || server == "?" || server == "." {
            return None;
        }
        Some(Self {
            anchor: format!("unc:{server}\\{share}"),
            components: normalize_windows_components(&parts.collect::<Vec<_>>().join("\\")),
        })
    }

    fn has_prefix(&self, prefix: &Self) -> bool {
        self.anchor == prefix.anchor
            && self.components.len() >= prefix.components.len()
            && self
                .components
                .iter()
                .zip(&prefix.components)
                .all(|(candidate, prefix)| candidate.eq_ignore_ascii_case(prefix))
    }
}

fn has_unsupported_windows_absolute_prefix(value: &str) -> bool {
    let normalized = value.replace('/', "\\");
    normalized.starts_with("\\\\?\\") || normalized.starts_with("\\\\.\\")
}

fn normalize_windows_components(value: &str) -> Vec<String> {
    let mut components = Vec::new();
    for component in value.split('\\') {
        match component {
            "" | "." => {}
            ".." => {
                components.pop();
            }
            component => components.push(component.to_owned()),
        }
    }
    components
}

fn lexical_normalize(path: &Path) -> Result<PathBuf, String> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return Err(format!(
                        "path escapes the machine workspace: {}",
                        path.display()
                    ));
                }
            }
            Component::Normal(component) => normalized.push(component),
        }
    }
    Ok(normalized)
}

fn require_under_workspace(path: &Path, workspace_root: &Path) -> Result<(), String> {
    if is_under_workspace(path, workspace_root)? {
        return Ok(());
    }
    Err(format!(
        "path is outside workspace_root: {}",
        path.display()
    ))
}

fn is_under_workspace(path: &Path, workspace_root: &Path) -> Result<bool, String> {
    let workspace_root = lexical_normalize(workspace_root)?;
    Ok(path == workspace_root || path.starts_with(&workspace_root))
}

fn strip_component_prefix<'a>(components: &'a [String], prefix: &[String]) -> Option<&'a [String]> {
    if components.len() < prefix.len() {
        return None;
    }
    if !components
        .iter()
        .zip(prefix)
        .all(|(left, right)| left == right)
    {
        return None;
    }
    Some(&components[prefix.len()..])
}

fn path_buf_from_components(components: &[String]) -> PathBuf {
    let mut path = PathBuf::new();
    for component in components {
        path.push(component);
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    mod when_a_posix_alias_matches_the_host_root_prefix_textually_only {
        use super::*;

        #[test]
        fn it_rejects_the_path() {
            let request: MachineRunRequest = serde_json::from_value(serde_json::json!({
                "protocol_version": MACHINE_PROTOCOL_VERSION,
                "workspace_root": "/workspace",
                "host_path_mapping": {
                    "kind": "alias",
                    "host_root": "/repo",
                    "style": "posix",
                },
                "argv": [],
            }))
            .unwrap();

            let error = request
                .remap_path(Path::new("/repo2/src/lib.rs"), request.workspace_root())
                .unwrap_err();

            assert!(error.contains("outside host_path_mapping.host_root"));
        }
    }

    mod when_a_windows_drive_path_is_under_the_host_root {
        use super::*;

        #[test]
        fn it_rewrites_the_suffix_under_the_workspace_root() {
            let request: MachineRunRequest = serde_json::from_value(serde_json::json!({
                "protocol_version": MACHINE_PROTOCOL_VERSION,
                "workspace_root": "/workspace",
                "host_path_mapping": {
                    "kind": "alias",
                    "host_root": "C:\\repo",
                    "style": "windows",
                },
                "argv": [],
            }))
            .unwrap();

            let path = request
                .remap_path(Path::new("C:\\repo\\src\\lib.rs"), request.workspace_root())
                .unwrap();

            assert_eq!(path, PathBuf::from("/workspace/src/lib.rs"));
        }
    }

    mod when_a_windows_device_path_is_passed {
        use super::*;

        #[test]
        fn it_rejects_the_unsupported_prefix() {
            let request: MachineRunRequest = serde_json::from_value(serde_json::json!({
                "protocol_version": MACHINE_PROTOCOL_VERSION,
                "workspace_root": "/workspace",
                "host_path_mapping": {
                    "kind": "alias",
                    "host_root": "C:\\repo",
                    "style": "windows",
                },
                "argv": [],
            }))
            .unwrap();

            let error = request
                .remap_path(
                    Path::new(r"\\?\C:\repo\src\lib.rs"),
                    request.workspace_root(),
                )
                .unwrap_err();

            assert!(error.contains("unsupported Windows path prefix"));
        }
    }
}
