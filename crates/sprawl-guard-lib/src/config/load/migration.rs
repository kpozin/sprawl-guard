use std::path::Path;

use toml_edit::DocumentMut;

use super::{
    AuthoredConfigPath, ConfigSchemaVersion, LoadOptions, PartialConfig, Result, SprawlError,
    deserialize_config, discover_config, parse_config_value, read_config_text, resolve_config_root,
    root_schema_version, validate_partial_config,
};

/// Prepared upgrade for one authored root configuration.
pub struct PreparedConfigUpgrade {
    path: AuthoredConfigPath,
    upgrade: ConfigUpgrade,
}

impl PreparedConfigUpgrade {
    /// Separates the authored path from its required upgrade state.
    pub fn into_parts(self) -> (AuthoredConfigPath, ConfigUpgrade) {
        (self.path, self.upgrade)
    }
}

/// Whether an authored root configuration is current or requires replacement.
pub enum ConfigUpgrade {
    /// The authored config already uses the current structural schema and directive.
    Current,
    /// The authored config must be replaced with this validated migrated document.
    Required(MigratedAuthoredConfigDocument),
}

/// Formatting-preserving authored config migrated to the current schema.
pub struct MigratedAuthoredConfigDocument(String);

impl std::fmt::Display for MigratedAuthoredConfigDocument {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Loads and validates an authored config using its declared schema before preparing an upgrade.
///
/// Unlike normal config loading, this entry point accepts supported legacy schema versions and
/// retains authored formatting only for documents that need to be rewritten.
pub fn prepare_config_upgrade(options: &LoadOptions) -> Result<Option<PreparedConfigUpgrade>> {
    let root = resolve_config_root(options)?;
    let Some(path) = discover_config(&root, options.config.as_deref())? else {
        return Ok(None);
    };
    let path = AuthoredConfigPath::new(path)?;
    let contents = read_config_text(path.as_path())?;
    let value = parse_config_value(path.as_path(), &contents)?;
    let version = root_schema_version(&value)?;

    let upgrade = match version {
        ConfigSchemaVersion::V1 => {
            let migrated = migrate_v1_document(path.as_path(), &contents)?;
            validate_migrated_current_config(path.as_path(), &migrated)?;
            ConfigUpgrade::Required(migrated)
        }
        ConfigSchemaVersion::V2 => {
            let partial = deserialize_config::<PartialConfig>(path.as_path(), value)?;
            validate_partial_config(partial)?;
            if has_current_schema_directive(&contents) {
                ConfigUpgrade::Current
            } else {
                let migrated = repair_schema_directive(path.as_path(), &contents)?;
                validate_migrated_current_config(path.as_path(), &migrated)?;
                ConfigUpgrade::Required(migrated)
            }
        }
    };

    Ok(Some(PreparedConfigUpgrade { path, upgrade }))
}

fn validate_migrated_current_config(
    path: &Path,
    migrated: &MigratedAuthoredConfigDocument,
) -> Result<()> {
    let value = parse_config_value(path, &migrated.0)?;
    let version = root_schema_version(&value)?;
    if version != ConfigSchemaVersion::CURRENT {
        return Err(SprawlError::UnsupportedConfigSchemaVersion {
            version: version.into(),
        });
    }
    validate_partial_config(deserialize_config(path, value)?)
}

/// Version 1 key renamed in version 2, located by its table path from the document root.
struct RenamedKeyV1 {
    table: &'static [&'static str],
    from: &'static str,
    to: &'static str,
}

/// Version 1 keys whose version 2 names differ, outside per-language and per-group tables.
const RENAMED_ROOT_KEYS_V1: [RenamedKeyV1; 5] = [
    RenamedKeyV1 {
        table: &["limits"],
        from: DIRECT_FILES_KEY_V1,
        to: DIRECT_FILES_KEY_V2,
    },
    RenamedKeyV1 {
        table: &["limits", "max_lines_per_file"],
        from: "included_line_types",
        to: "count_lines",
    },
    RenamedKeyV1 {
        table: &["files"],
        from: "exclude",
        to: "extend_exclude",
    },
    RenamedKeyV1 {
        table: &["tests"],
        from: "check_path_tests",
        to: "check",
    },
    RenamedKeyV1 {
        table: &["traversal"],
        from: "error_policy",
        to: "unreadable_paths",
    },
];

/// Version 1 name of the per-directory direct-file limit, used at the root, in language groups,
/// and in path overrides.
const DIRECT_FILES_KEY_V1: &str = "max_leaf_files_per_directory";

/// Version 2 name of the per-directory direct-file limit.
const DIRECT_FILES_KEY_V2: &str = "max_direct_files_per_directory";

/// Version 1 key dropped in version 2 because the setting no longer exists.
const REMOVED_KEYS_V1: [(&str, &str); 2] = [("output", "color"), ("traversal", "max_depth")];

/// Version 1 line type that version 2 counts implicitly and omits from `count_lines`.
const IMPLICIT_LINE_TYPE_V1: &str = "code";

/// Rewrites a version 1 document's keys to their version 2 names.
///
/// The result is validated as a current configuration afterward, so this transform renames keys
/// and leaves value validation to the current schema. Validation errors therefore name the version
/// 2 keys, and values of the removed keys are discarded without being checked.
fn migrate_v1_document(path: &Path, contents: &str) -> Result<MigratedAuthoredConfigDocument> {
    rewrite_authored_config(path, contents, |document| {
        let root = document.as_table_mut();
        for (table, key) in REMOVED_KEYS_V1 {
            if let Some(table) = table_at(root, &[table]) {
                table.remove(key);
            }
        }
        for renamed in &RENAMED_ROOT_KEYS_V1 {
            if let Some(table) = table_at(root, renamed.table) {
                rename_key(table, renamed.from, renamed.to);
            }
        }
        if let Some(line_types) = table_at(root, &["limits", "max_lines_per_file"])
            .and_then(|table| table.get_mut("count_lines"))
        {
            drop_implicit_line_type(line_types);
        }
        migrate_v1_languages(root);
        if let Some(groups) = table_at(root, &["language_groups"]) {
            for (_, group) in groups.iter_mut() {
                if let Some(limits) = group
                    .as_table_like_mut()
                    .and_then(|group| table_at(group, &["limits"]))
                {
                    rename_key(limits, DIRECT_FILES_KEY_V1, DIRECT_FILES_KEY_V2);
                }
            }
        }
        match root.get_mut("overrides") {
            Some(toml_edit::Item::ArrayOfTables(overrides)) => {
                for path_override in overrides.iter_mut() {
                    rename_key(path_override, DIRECT_FILES_KEY_V1, DIRECT_FILES_KEY_V2);
                }
            }
            Some(toml_edit::Item::Value(toml_edit::Value::Array(overrides))) => {
                for path_override in overrides.iter_mut() {
                    if let Some(path_override) = path_override.as_inline_table_mut() {
                        rename_key(path_override, DIRECT_FILES_KEY_V1, DIRECT_FILES_KEY_V2);
                    }
                }
            }
            _ => {}
        }
    })
}

/// Renames per-language `additional_globs` and `included_line_types` keys.
fn migrate_v1_languages(root: &mut dyn toml_edit::TableLike) {
    let Some(languages) = table_at(root, &["languages"]) else {
        return;
    };
    for (_, language) in languages.iter_mut() {
        let Some(language) = language.as_table_like_mut() else {
            continue;
        };
        rename_key(language, "additional_globs", "extend_include");
        if let Some(limits) = table_at(language, &["limits", "max_lines_per_file"]) {
            rename_key(limits, "included_line_types", "count_lines");
            if let Some(line_types) = limits.get_mut("count_lines") {
                drop_implicit_line_type(line_types);
            }
        }
    }
}

/// Returns the table-like item at `path`, whether written as a table, inline table, or dotted key.
pub(super) fn table_at<'a>(
    table: &'a mut dyn toml_edit::TableLike,
    path: &[&str],
) -> Option<&'a mut dyn toml_edit::TableLike> {
    path.iter().try_fold(table, |table, key| {
        table
            .get_mut(key)
            .and_then(toml_edit::Item::as_table_like_mut)
    })
}

/// Renames one key in place, keeping its comments and its position among sibling keys.
///
/// When both names are present, the old key stays so that validation rejects it as unknown
/// instead of silently dropping one of the two values.
pub(super) fn rename_key(table: &mut dyn toml_edit::TableLike, from: &str, to: &str) {
    if table.contains_key(to) {
        return;
    }
    let Some(from_key) = table.key(from).cloned() else {
        return;
    };
    let following: Vec<String> = table
        .iter()
        .map(|(key, _)| key.to_owned())
        .skip_while(|key| key != from)
        .skip(1)
        .collect();
    let item = table
        .remove(from)
        .expect("a key found by name must be removable");
    let to_key = toml_edit::Key::new(to)
        .with_leaf_decor(from_key.leaf_decor().clone())
        .with_dotted_decor(from_key.dotted_decor().clone());
    table.entry_format(&to_key).or_insert(item);
    for key in following {
        let formatted = table.key(&key).cloned();
        if let (Some(formatted), Some(item)) = (formatted, table.remove(&key)) {
            table.entry_format(&formatted).or_insert(item);
        }
    }
}

/// Removes `"code"` from a version 1 line-type array, since version 2 always counts code.
fn drop_implicit_line_type(item: &mut toml_edit::Item) {
    let Some(array) = item.as_array_mut() else {
        return;
    };
    // The first element's spacing follows the opening bracket, so it passes to the new first
    // element instead of leaving `[ "comments"]`.
    let first_prefix = array
        .get(0)
        .filter(|value| value.as_str() == Some(IMPLICIT_LINE_TYPE_V1))
        .map(|value| value.decor().prefix().cloned());
    array.retain(|value| value.as_str() != Some(IMPLICIT_LINE_TYPE_V1));
    if let (Some(prefix), Some(first)) = (first_prefix, array.get_mut(0)) {
        match prefix {
            Some(prefix) => first.decor_mut().set_prefix(prefix),
            None => first.decor_mut().clear(),
        }
    }
}

fn repair_schema_directive(path: &Path, contents: &str) -> Result<MigratedAuthoredConfigDocument> {
    rewrite_authored_config(path, contents, |_| {})
}

fn rewrite_authored_config(
    path: &Path,
    contents: &str,
    mutate: impl FnOnce(&mut DocumentMut),
) -> Result<MigratedAuthoredConfigDocument> {
    let line_ending = ConfigLineEnding::from_source(contents);
    let first_line_end = contents
        .find('\n')
        .map_or(contents.len(), |index| index + 1);
    let first_line = contents[..first_line_end].trim_end_matches(['\r', '\n']);
    let source_without_directive = if is_schema_directive(first_line) {
        &contents[first_line_end..]
    } else {
        contents
    };
    let mut document = source_without_directive
        .parse::<DocumentMut>()
        .map_err(|source| SprawlError::ParseEditableToml {
            path: path.to_path_buf(),
            source,
        })?;
    mutate(&mut document);
    document["schema_version"] = toml_edit::value(i64::from(ConfigSchemaVersion::CURRENT));
    document.as_table_mut().decor_mut().set_prefix(format!(
        "{}\n",
        ConfigSchemaVersion::CURRENT.schema_directive()
    ));
    let rendered = document.to_string();
    let rendered = match line_ending {
        ConfigLineEnding::Lf => rendered,
        ConfigLineEnding::CrLf => rendered.replace('\n', "\r\n"),
    };
    Ok(MigratedAuthoredConfigDocument(rendered))
}

fn has_current_schema_directive(contents: &str) -> bool {
    let first_line_end = contents
        .find('\n')
        .map_or(contents.len(), |index| index + 1);
    contents[..first_line_end].trim_end_matches(['\r', '\n'])
        == ConfigSchemaVersion::CURRENT.schema_directive().to_string()
}

fn is_schema_directive(line: &str) -> bool {
    line.strip_prefix("#:schema")
        .is_some_and(|suffix| suffix.is_empty() || suffix.starts_with(char::is_whitespace))
}

#[derive(Clone, Copy)]
enum ConfigLineEnding {
    Lf,
    CrLf,
}

impl ConfigLineEnding {
    fn from_source(source: &str) -> Self {
        source.find('\n').map_or(Self::Lf, |index| {
            if source[..index].ends_with('\r') {
                Self::CrLf
            } else {
                Self::Lf
            }
        })
    }
}
