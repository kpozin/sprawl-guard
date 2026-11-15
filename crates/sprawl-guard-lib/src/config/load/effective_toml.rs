//! Renders a resolved configuration with the effective values that checks use.

use toml_edit::{DocumentMut, Item, TableLike};

use super::Config;
use super::migration::{rename_key, table_at};

/// Line type that checks always count, and that authored `count_lines` lists leave out.
const ALWAYS_COUNTED_LINE_TYPE: &str = "code";

/// Renders `config` as TOML that spells out effective values instead of authored shorthand.
///
/// Authored configuration lists only the extra line types in `count_lines`, adds to the built-in
/// file excludes with `extend_exclude`, and adds language globs with `extend_include`. This output
/// instead lists every counted line type, names the merged exclude list `exclude`, and folds
/// `extend_include` into a language's explicit `include`. A language that matches its bundled paths
/// keeps `extend_include`, because bundled paths are file extensions and names rather than globs.
///
/// The output describes what checks do and is not valid authored configuration; `config export`
/// writes that form.
pub fn render_effective_config_toml(config: &Config) -> Result<String, toml::ser::Error> {
    let rendered = toml::to_string_pretty(&config.materialized_for_inspection())?;
    let mut document = rendered
        .parse::<DocumentMut>()
        .expect("serialized configuration must parse as TOML");
    let root = document.as_table_mut();
    list_counted_lines_at(root);
    if let Some(files) = table_at(root, &["files"]) {
        rename_key(files, "extend_exclude", "exclude");
    }
    if let Some(languages) = table_at(root, &["languages"]) {
        for (_, language) in languages.iter_mut() {
            if let Some(language) = language.as_table_like_mut() {
                fold_extend_include(language);
                list_counted_lines_at(language);
            }
        }
    }
    Ok(document.to_string())
}

/// Adds the always-counted line type to the `count_lines` under `limits.max_lines_per_file`.
fn list_counted_lines_at(table: &mut dyn TableLike) {
    if let Some(count_lines) = table_at(table, &["limits", "max_lines_per_file"])
        .and_then(|limits| limits.get_mut("count_lines"))
        .and_then(Item::as_array_mut)
    {
        count_lines.insert(0, ALWAYS_COUNTED_LINE_TYPE);
        count_lines.fmt();
    }
}

/// Appends a language's `extend_include` globs to its explicit `include`, when it has one.
fn fold_extend_include(language: &mut dyn TableLike) {
    if !language.contains_key("include") {
        return;
    }
    let extensions = language
        .remove("extend_include")
        .and_then(|item| item.as_array().cloned())
        .unwrap_or_default();
    let include = language
        .get_mut("include")
        .and_then(Item::as_array_mut)
        .expect("a resolved language include must be an array");
    include.extend(extensions);
    include.fmt();
}

#[cfg(test)]
mod tests {
    use indoc::indoc;

    use super::*;
    use crate::test_support::{load_config, write_config};

    fn effective_config(authored: &str) -> toml::Table {
        let tempdir = tempfile::tempdir().unwrap();
        write_config(tempdir.path(), authored);
        let rendered = render_effective_config_toml(&load_config(tempdir.path())).unwrap();
        toml::from_str(&rendered).unwrap()
    }

    fn strings(value: &toml::Value) -> Vec<&str> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect()
    }

    mod render_effective_config_toml {
        use super::*;

        mod when_only_code_lines_count {
            use super::*;

            #[test]
            fn it_lists_code_at_the_top_level_and_for_each_language() {
                let config = effective_config("");

                assert_eq!(
                    strings(&config["limits"]["max_lines_per_file"]["count_lines"]),
                    ["code"]
                );
                assert_eq!(
                    strings(
                        &config["languages"]["Rust"]["limits"]["max_lines_per_file"]["count_lines"]
                    ),
                    ["code"]
                );
            }
        }

        mod when_extra_line_types_count {
            use super::*;

            #[test]
            fn it_lists_code_before_them() {
                let config = effective_config(indoc! {r#"
                    [limits.max_lines_per_file]
                    count_lines = ["blank_lines"]
                "#});

                assert_eq!(
                    strings(&config["limits"]["max_lines_per_file"]["count_lines"]),
                    ["code", "blank_lines"]
                );
                assert_eq!(
                    strings(
                        &config["languages"]["Markdown"]["limits"]["max_lines_per_file"]["count_lines"]
                    ),
                    ["code", "comments", "blank_lines"]
                );
            }
        }

        mod when_file_excludes_are_extended {
            use super::*;

            #[test]
            fn it_names_the_merged_list_exclude() {
                let config = effective_config(indoc! {r#"
                    [files]
                    extend_exclude = ["generated/**"]
                "#});

                let files = config["files"].as_table().unwrap();
                assert!(!files.contains_key("extend_exclude"));
                let excludes = strings(&files["exclude"]);
                assert!(excludes.contains(&"**/pnpm-lock.yaml"));
                assert!(excludes.contains(&"generated/**"));
            }
        }

        mod when_a_language_has_an_explicit_include {
            use super::*;

            #[test]
            fn it_folds_extend_include_into_include() {
                let config = effective_config(indoc! {r#"
                    [languages.rust]
                    include = ["src/**/*.rs"]
                    extend_include = ["build/*.rs"]
                "#});

                let rust = config["languages"]["Rust"].as_table().unwrap();
                assert_eq!(strings(&rust["include"]), ["src/**/*.rs", "build/*.rs"]);
                assert!(!rust.contains_key("extend_include"));
            }
        }

        mod when_a_language_uses_its_bundled_paths {
            use super::*;

            #[test]
            fn it_keeps_extend_include() {
                let config = effective_config(indoc! {r#"
                    [languages.rust]
                    extend_include = ["build/*.rs.in"]
                "#});

                let rust = config["languages"]["Rust"].as_table().unwrap();
                assert!(!rust.contains_key("include"));
                assert_eq!(strings(&rust["extend_include"]), ["build/*.rs.in"]);
            }
        }
    }
}
