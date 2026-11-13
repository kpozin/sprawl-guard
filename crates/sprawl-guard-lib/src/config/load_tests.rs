#![cfg(test)]

use std::fs;
use std::path::Path;

use indoc::indoc;
use tokei::LanguageType;

use super::*;
use crate::config::{
    ConfigSchemaVersion, LanguageConfig, SourceFileByteLimit, SymlinkTraversal,
    TraversalEntryLimit, UnreadablePathPolicy,
};
use crate::{
    CountLines, CustomLanguageGroupId, DirectFileLimit, DirectFiles, GlobPattern, LanguageGroupId,
};

fn load_config_fixture(root: &Path) -> Result<LoadedConfig> {
    let config_path = root.join(PRIMARY_CONFIG_FILE);
    if config_path.exists() {
        let contents = fs::read_to_string(&config_path).unwrap();
        fs::write(config_path, format!("schema_version = 2\n{contents}")).unwrap();
    }
    load_raw_config(root)
}

fn load_raw_config(root: &Path) -> Result<LoadedConfig> {
    load_resolved_config(&LoadOptions {
        root: root.to_path_buf(),
        config: None,
    })
}

fn assert_patterns(actual: &[GlobPattern], expected: &[&str]) {
    assert_eq!(
        actual.iter().map(GlobPattern::as_str).collect::<Vec<_>>(),
        expected
    );
}

fn language<'config>(config: &'config Config, id: &str) -> &'config LanguageConfig {
    config.languages.get(&LanguageId::new(id).unwrap()).unwrap()
}

mod when_config_schema_version_is_read {
    use super::*;

    #[test]
    fn it_rejects_a_missing_version_with_a_typed_error() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(tempdir.path().join(PRIMARY_CONFIG_FILE), "").unwrap();

        let error = load_raw_config(tempdir.path()).unwrap_err();

        assert!(matches!(error, SprawlError::MissingConfigSchemaVersion));
    }

    #[test]
    fn it_rejects_version_one_until_upgraded() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            "schema_version = 1\n",
        )
        .unwrap();

        let error = load_raw_config(tempdir.path()).unwrap_err();

        assert!(matches!(
            error,
            SprawlError::UnsupportedConfigSchemaVersion { version: 1 }
        ));
    }

    #[test]
    fn it_accepts_version_two() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            "schema_version = 2\n",
        )
        .unwrap();

        let loaded = load_raw_config(tempdir.path()).unwrap();

        assert_eq!(loaded.config.schema_version, ConfigSchemaVersion::V2);
    }

    #[test]
    fn it_rejects_an_unsupported_numeric_version_with_a_typed_error() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            "schema_version = 3\n",
        )
        .unwrap();

        let error = load_raw_config(tempdir.path()).unwrap_err();

        assert!(matches!(
            error,
            SprawlError::UnsupportedConfigSchemaVersion { version: 3 }
        ));
    }

    #[test]
    fn it_rejects_a_nonnumeric_version_with_a_typed_error() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            "schema_version = \"2\"\n",
        )
        .unwrap();

        let error = load_raw_config(tempdir.path()).unwrap_err();

        assert!(matches!(error, SprawlError::InvalidConfigSchemaVersionType));
    }

    #[test]
    fn it_assigns_the_current_version_when_no_config_file_exists() {
        let tempdir = tempfile::tempdir().unwrap();

        let loaded = load_raw_config(tempdir.path()).unwrap();

        assert_eq!(loaded.config.schema_version, ConfigSchemaVersion::CURRENT);
    }

    #[test]
    fn it_serializes_the_version_before_root_tables() {
        let serialized = toml::to_string_pretty(&built_in_config()).unwrap();

        assert!(serialized.starts_with("schema_version = 2\n"));
    }
}

#[cfg(target_os = "linux")]
mod non_utf8_authored_config_path {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt as _;
    use std::path::PathBuf;

    use super::*;

    fn non_utf8_config_fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let tempdir = tempfile::tempdir().unwrap();
        let relative_path = PathBuf::from(OsString::from_vec(b"config-\xff.toml".to_vec()));
        let absolute_path = tempdir.path().join(&relative_path);
        fs::write(&absolute_path, "schema_version = 2\n").unwrap();
        (tempdir, relative_path, absolute_path)
    }

    mod when_loading_resolved_config {
        use super::*;

        #[test]
        fn it_rejects_the_path_before_structured_provenance_can_be_lossy() {
            let (tempdir, relative_path, absolute_path) = non_utf8_config_fixture();

            let error = load_resolved_config(&LoadOptions {
                root: tempdir.path().to_path_buf(),
                config: Some(relative_path),
            })
            .unwrap_err();

            assert!(matches!(
                error,
                SprawlError::NonUtf8ConfigPath { path } if path == absolute_path
            ));
        }
    }

    mod when_preparing_a_config_upgrade {
        use super::*;

        #[test]
        fn it_rejects_the_path_before_migration_output_can_be_lossy() {
            let (tempdir, relative_path, absolute_path) = non_utf8_config_fixture();

            let result = prepare_config_upgrade(&LoadOptions {
                root: tempdir.path().to_path_buf(),
                config: Some(relative_path),
            });

            assert!(matches!(
                result,
                Err(SprawlError::NonUtf8ConfigPath { path }) if path == absolute_path
            ));
        }
    }
}

mod when_config_is_minimal {
    use super::*;

    #[test]
    fn it_uses_the_default_directory_direct_files_limits() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            "schema_version = 2\n",
        )
        .unwrap();

        let loaded = load_raw_config(tempdir.path()).unwrap();

        let expected = DirectFileLimit::Finite(DirectFiles::new(10).unwrap());
        assert_eq!(
            loaded
                .config
                .limits
                .max_direct_files_per_directory
                .production,
            expected
        );
        assert_eq!(
            loaded.config.limits.max_direct_files_per_directory.test,
            expected
        );
    }

    #[test]
    fn it_merges_language_defaults() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [languages.rust]
                enabled = true
            "#},
        )
        .unwrap();

        let loaded = load_config_fixture(tempdir.path()).unwrap();
        let rust = language(&loaded.config, "rust");

        assert!(rust.is_enabled());
        assert_eq!(rust.common().include, None);
        assert!(rust.common().extend_include.is_empty());
        assert_patterns(&rust.common().test_globs, &["benches/**"]);
        assert_patterns(
            &loaded.config.tests.globs,
            &[
                "tests/**",
                "**/tests/**",
                "test/**",
                "**/test/**",
                "spec/**",
                "**/spec/**",
                "__tests__/**",
                "**/__tests__/**",
                "__mocks__/**",
                "**/__mocks__/**",
                "**/*.test.*",
                "**/*.spec.*",
                "**/*_test.*",
                "**/*_spec.*",
            ],
        );
        assert!(rust.rust().unwrap().cfg_test_exclusion.is_enabled());
    }

    #[test]
    fn it_includes_every_tokei_language() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(tempdir.path().join(PRIMARY_CONFIG_FILE), "").unwrap();

        let loaded = load_config_fixture(tempdir.path()).unwrap();

        assert_eq!(loaded.config.languages.len(), LanguageType::list().len());
        for (language, _) in LanguageType::list() {
            assert!(
                loaded
                    .config
                    .languages
                    .contains_key(&LanguageId::from_tokei(*language)),
                "missing default config for {}",
                language.name()
            );
            assert!(
                loaded
                    .config
                    .languages
                    .get(&LanguageId::from_tokei(*language))
                    .unwrap()
                    .is_enabled(),
                "{} should be enabled by default",
                language.name()
            );
        }
    }

    #[test]
    fn it_does_not_materialize_bundled_language_globs_when_serialized() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(tempdir.path().join(PRIMARY_CONFIG_FILE), "").unwrap();
        let loaded = load_config_fixture(tempdir.path()).unwrap();

        let serialized = toml::Value::try_from(&loaded.config).unwrap();
        let rust = serialized
            .get("languages")
            .and_then(|languages| languages.get("Rust"))
            .and_then(toml::Value::as_table)
            .unwrap();

        assert!(!rust.contains_key("include"));
    }
}

mod when_a_ratchet_document_is_used_as_config {
    use super::*;

    #[test]
    fn it_rejects_ratchet_entries_as_unknown_config_fields() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                schema_version = 2

                [[files]]
                path = "src/lib.rs"
                role = "production"
                max_lines = 2
            "#},
        )
        .unwrap();

        let error = load_raw_config(tempdir.path()).unwrap_err();

        assert!(matches!(error, SprawlError::ParseToml { .. }));
    }
}

mod when_language_config_is_partial {
    use super::*;

    #[test]
    fn it_can_disable_a_language() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [languages.rust]
                enabled = false
            "#},
        )
        .unwrap();

        let loaded = load_config_fixture(tempdir.path()).unwrap();

        assert!(!language(&loaded.config, "rust").is_enabled());
    }

    #[test]
    fn it_replaces_bundled_paths_with_an_explicit_include() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [languages.rust]
                include = ["generated/**/*.rust"]
            "#},
        )
        .unwrap();

        let loaded = load_config_fixture(tempdir.path()).unwrap();
        let include = language(&loaded.config, "rust")
            .common()
            .include
            .as_deref()
            .unwrap();

        assert_patterns(include, &["generated/**/*.rust"]);
    }

    #[test]
    fn it_adds_paths_without_replacing_bundled_paths() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [languages.rust]
                extend_include = ["generated/**/*.rust"]
            "#},
        )
        .unwrap();

        let loaded = load_config_fixture(tempdir.path()).unwrap();
        let rust = language(&loaded.config, "rust").common();

        assert_eq!(rust.include, None);
        assert_patterns(&rust.extend_include, &["generated/**/*.rust"]);
    }

    #[test]
    fn it_merges_all_language_file_limit_fields() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [languages.rust.limits.max_lines_per_file]
                count_lines = ["comments"]
                production = 500
                test = 1000
            "#},
        )
        .unwrap();

        let loaded = load_config_fixture(tempdir.path()).unwrap();
        let limits = &language(&loaded.config, "rust")
            .common()
            .limits
            .max_lines_per_file;

        assert_eq!(limits.count_lines, Some(CountLines::CodeAndComments));
        assert_eq!(limits.production.unwrap().get(), 500);
        assert_eq!(limits.test.unwrap().get(), 1000);
    }

    #[test]
    fn it_keeps_language_local_exclusions() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [languages.rust]
                exclude = ["vendor/**"]
            "#},
        )
        .unwrap();

        let loaded = load_config_fixture(tempdir.path()).unwrap();

        assert_patterns(
            &language(&loaded.config, "rust").common().exclude,
            &["vendor/**"],
        );
    }

    #[test]
    fn it_allows_a_disabled_language_without_included_paths() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [languages.rust]
                enabled = false
                include = []
            "#},
        )
        .unwrap();

        let loaded = load_config_fixture(tempdir.path()).unwrap();

        assert!(!language(&loaded.config, "rust").is_enabled());
    }
}

mod when_config_has_unknown_keys {
    use super::*;

    #[test]
    fn it_rejects_the_config() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(tempdir.path().join(PRIMARY_CONFIG_FILE), "unknown = true\n").unwrap();

        let error = load_config_fixture(tempdir.path()).unwrap_err();

        assert!(error.to_string().contains("unknown field"));
    }
}

mod when_tests_config_is_partial {
    use super::*;

    #[test]
    fn it_merges_global_test_globs() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [tests]
                globs = ["custom-tests/**", "**/*.fixture.*"]
            "#},
        )
        .unwrap();

        let loaded = load_config_fixture(tempdir.path()).unwrap();

        assert_patterns(
            &loaded.config.tests.globs,
            &["custom-tests/**", "**/*.fixture.*"],
        );
        assert!(loaded.config.tests.check);
    }
}

mod when_traversal_config_is_partial {
    use super::*;

    #[test]
    fn it_merges_traversal_policy() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [traversal]
                follow_symlinks = "files-and-directories"
                same_file_system = false
                max_entries = 1234
                max_file_bytes = 4096
                unreadable_paths = "warn"
            "#},
        )
        .unwrap();

        let loaded = load_config_fixture(tempdir.path()).unwrap();

        assert_eq!(
            loaded.config.traversal.follow_symlinks,
            SymlinkTraversal::FilesAndDirectories
        );
        assert!(!loaded.config.traversal.same_file_system);
        assert_eq!(
            loaded.config.traversal.max_entries,
            TraversalEntryLimit::new(1234).unwrap()
        );
        assert_eq!(
            loaded.config.traversal.max_file_bytes,
            SourceFileByteLimit::new(4096).unwrap()
        );
        assert_eq!(
            loaded.config.traversal.unreadable_paths,
            UnreadablePathPolicy::Warn
        );
    }
}

mod when_limits_config_is_partial {
    use super::*;

    #[test]
    fn it_merges_file_line_limits() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [limits.max_lines_per_file]
                count_lines = ["blank_lines", "comments"]
                production = 500
                test = 900
            "#},
        )
        .unwrap();

        let loaded = load_config_fixture(tempdir.path()).unwrap();

        assert_eq!(
            loaded.config.limits.max_lines_per_file.production.get(),
            500
        );
        assert_eq!(loaded.config.limits.max_lines_per_file.test.get(), 900);
        assert_eq!(
            loaded.config.limits.max_lines_per_file.count_lines,
            CountLines::All
        );
    }

    #[test]
    fn it_merges_directory_direct_files_limits() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [limits]
                max_direct_files_per_directory = { production = 10, test = 20 }
            "#},
        )
        .unwrap();

        let loaded = load_config_fixture(tempdir.path()).unwrap();

        assert_eq!(
            loaded
                .config
                .limits
                .max_direct_files_per_directory
                .production,
            DirectFileLimit::Finite(DirectFiles::new(10).unwrap())
        );
        assert_eq!(
            loaded.config.limits.max_direct_files_per_directory.test,
            DirectFileLimit::Finite(DirectFiles::new(20).unwrap())
        );
    }

    #[test]
    fn it_accepts_unlimited_direct_files_for_both_roles() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [limits]
                max_direct_files_per_directory = { production = "unlimited", test = "unlimited" }
            "#},
        )
        .unwrap();

        let loaded = load_config_fixture(tempdir.path()).unwrap();

        assert_eq!(
            loaded
                .config
                .limits
                .max_direct_files_per_directory
                .production,
            DirectFileLimit::Unlimited
        );
        assert_eq!(
            loaded.config.limits.max_direct_files_per_directory.test,
            DirectFileLimit::Unlimited
        );
    }

    #[test]
    fn it_loads_the_final_inline_path_override_shape() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [[overrides]]
                paths = ["packages/web/**"]
                max_lines_per_file = { production = 600, test = 1200 }
                max_direct_files_per_directory = { production = 20, test = "unlimited" }
                reason = "The web package colocates route modules"
            "#},
        )
        .unwrap();

        let loaded = load_config_fixture(tempdir.path()).unwrap();
        let limits = &loaded.config.overrides[0].limits;

        assert_eq!(limits.production_loc_per_file().unwrap().get(), 600);
        assert_eq!(limits.test_loc_per_file().unwrap().get(), 1200);
        assert_eq!(
            limits.production_direct_files_per_directory(),
            Some(DirectFileLimit::Finite(DirectFiles::new(20).unwrap()))
        );
        assert_eq!(
            limits.test_direct_files_per_directory(),
            Some(DirectFileLimit::Unlimited)
        );
    }
}

mod when_language_groups_are_configured {
    use super::*;

    #[test]
    fn it_assigns_custom_members_and_inherits_omitted_global_limits() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [limits.max_direct_files_per_directory]
                production = 10
                test = 20

                [language_groups.frontend]
                languages = ["javascript", "typescript", "jsx", "tsx"]

                [language_groups.frontend.limits.max_direct_files_per_directory]
                production = 12
            "#},
        )
        .unwrap();

        let loaded = load_config_fixture(tempdir.path()).unwrap();
        let frontend = CustomLanguageGroupId::try_new("frontend".to_owned()).unwrap();
        let group = loaded
            .config
            .language_groups()
            .custom()
            .get(&frontend)
            .unwrap();

        assert_eq!(
            group.limits.max_direct_files_per_directory,
            crate::config::MaxDirectFilesPerDirectoryLimits {
                production: DirectFileLimit::Finite(DirectFiles::new(12).unwrap()),
                test: DirectFileLimit::Finite(DirectFiles::new(20).unwrap()),
            }
        );
        for language in ["javascript", "typescript", "jsx", "tsx"] {
            assert_eq!(
                loaded
                    .config
                    .language_groups()
                    .group_for(LanguageId::new(language).unwrap()),
                &LanguageGroupId::Custom(frontend.clone())
            );
        }
        assert_eq!(
            loaded
                .config
                .language_groups()
                .group_for(LanguageId::new("rust").unwrap()),
            &LanguageGroupId::Source
        );
    }

    #[test]
    fn it_allows_disabled_languages_to_remain_custom_group_members() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [languages.javascript]
                enabled = false

                [language_groups.frontend]
                languages = ["javascript", "typescript"]
            "#},
        )
        .unwrap();

        let loaded = load_config_fixture(tempdir.path()).unwrap();

        assert_eq!(
            loaded
                .config
                .language_groups()
                .group_for(LanguageId::new("javascript").unwrap())
                .to_string(),
            "frontend"
        );
    }

    #[test]
    fn it_applies_explicit_documentation_limits_over_unlimited_defaults() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [language_groups.documentation.limits.max_direct_files_per_directory]
                production = 20
            "#},
        )
        .unwrap();

        let loaded = load_config_fixture(tempdir.path()).unwrap();

        assert_eq!(
            loaded
                .config
                .language_groups()
                .documentation()
                .limits
                .max_direct_files_per_directory,
            crate::config::MaxDirectFilesPerDirectoryLimits {
                production: DirectFileLimit::Finite(DirectFiles::new(20).unwrap()),
                test: DirectFileLimit::Unlimited,
            }
        );
        assert_eq!(
            loaded
                .config
                .language_groups()
                .group_for(LanguageId::new("markdown").unwrap()),
            &LanguageGroupId::Documentation
        );
    }

    #[test]
    fn it_serializes_authored_groups_without_the_assignment_index_or_source_group() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [language_groups.frontend]
                languages = ["javascript", "typescript"]
            "#},
        )
        .unwrap();
        let loaded = load_config_fixture(tempdir.path()).unwrap();

        let serialized = toml::Value::try_from(&loaded.config).unwrap();
        let groups = serialized
            .get("language_groups")
            .and_then(toml::Value::as_table)
            .unwrap();

        assert!(groups.contains_key("documentation"));
        assert!(groups.contains_key("frontend"));
        assert!(!groups.contains_key("source"));
        assert!(!groups.contains_key("custom"));
        assert!(!groups.contains_key("assignments"));
    }
}

mod when_config_toml_is_invalid {
    use super::*;

    #[test]
    fn it_reports_parse_failure() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(tempdir.path().join(PRIMARY_CONFIG_FILE), "[limits\n").unwrap();

        let error = load_config_fixture(tempdir.path()).unwrap_err();

        assert!(error.to_string().contains("failed to parse"));
    }
}

mod when_config_values_are_invalid {
    use super::*;

    fn assert_config_error_contains(config: &str, expected: &str) {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(tempdir.path().join(PRIMARY_CONFIG_FILE), config).unwrap();

        let error = load_config_fixture(tempdir.path()).unwrap_err();

        assert!(error.to_string().contains(expected));
    }

    #[test]
    fn it_rejects_invalid_globs() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [files]
                extend_exclude = ["["]
            "#},
        )
        .unwrap();

        let error = load_config_fixture(tempdir.path()).unwrap_err();

        assert!(error.to_string().contains("invalid glob pattern"));
    }

    #[test]
    fn it_rejects_zero_limits() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [limits.max_lines_per_file]
                production = 0
            "#},
        )
        .unwrap();

        let error = load_config_fixture(tempdir.path()).unwrap_err();

        assert!(error.to_string().contains("must be greater than zero"));
    }

    #[test]
    fn it_rejects_zero_traversal_entry_limit() {
        assert_config_error_contains(
            indoc! {r#"
                [traversal]
                max_entries = 0
            "#},
            "traversal max_entries must be greater than zero",
        );
    }

    #[test]
    fn it_rejects_zero_source_file_byte_limit() {
        assert_config_error_contains(
            indoc! {r#"
                [traversal]
                max_file_bytes = 0
            "#},
            "traversal max_file_bytes must be greater than zero",
        );
    }

    #[test]
    fn it_rejects_unknown_test_direct_files_keywords() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [limits]
                max_direct_files_per_directory = { test = "many" }
            "#},
        )
        .unwrap();

        let error = load_config_fixture(tempdir.path()).unwrap_err();

        assert!(error.to_string().contains("expected a positive integer"));
    }

    #[test]
    fn it_rejects_code_in_count_lines_because_code_always_counts() {
        assert_config_error_contains(
            indoc! {r#"
                [limits.max_lines_per_file]
                count_lines = ["code"]
            "#},
            "unknown variant `code`",
        );
    }

    #[test]
    fn it_rejects_duplicate_count_lines() {
        assert_config_error_contains(
            indoc! {r#"
                [limits.max_lines_per_file]
                count_lines = ["comments", "comments"]
            "#},
            "count_lines contains duplicate comments",
        );
    }

    #[test]
    fn it_rejects_parent_directory_ratchet_paths() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [ratchet]
                file = "../ratchet.toml"
            "#},
        )
        .unwrap();

        let error = load_config_fixture(tempdir.path()).unwrap_err();

        assert!(error.to_string().contains("must not contain '..'"));
    }

    #[test]
    fn it_rejects_windows_style_ratchet_paths() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [ratchet]
                file = "..\\ratchet.toml"
            "#},
        )
        .unwrap();

        let error = load_config_fixture(tempdir.path()).unwrap_err();

        assert!(error.to_string().contains("forward slash separators"));
    }

    #[test]
    fn it_rejects_windows_drive_ratchet_paths() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [ratchet]
                file = "C:\\ratchet.toml"
            "#},
        )
        .unwrap();

        let error = load_config_fixture(tempdir.path()).unwrap_err();

        assert!(error.to_string().contains("forward slash separators"));
    }

    #[test]
    fn it_rejects_windows_drive_paths_with_forward_slashes() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [ratchet]
                file = "C:/ratchet.toml"
            "#},
        )
        .unwrap();

        let error = load_config_fixture(tempdir.path()).unwrap_err();

        assert!(error.to_string().contains("must not contain ':'"));
    }

    #[test]
    fn it_rejects_legacy_color_in_current_config() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [output]
                color = "always"
            "#},
        )
        .unwrap();

        let error = load_config_fixture(tempdir.path()).unwrap_err();

        assert!(error.to_string().contains("unknown field `color`"));
    }

    #[test]
    fn it_rejects_unsupported_languages() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [languages.not-a-tokei-language]
                enabled = true
            "#},
        )
        .unwrap();

        let error = load_config_fixture(tempdir.path()).unwrap_err();

        assert!(error.to_string().contains("unsupported language ID"));
    }

    #[test]
    fn it_rejects_rust_specific_settings_for_other_languages() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [languages.typescript]
                enabled = true
                exclude_cfg_test = true
            "#},
        )
        .unwrap();

        let error = load_config_fixture(tempdir.path()).unwrap_err();

        assert!(error.to_string().contains("exclude_cfg_test"));
        assert!(error.to_string().contains("TypeScript"));
    }

    #[test]
    fn it_rejects_an_enabled_language_without_included_paths() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [languages.rust]
                include = []
            "#},
        )
        .unwrap();

        let error = load_config_fixture(tempdir.path()).unwrap_err();

        assert!(error.to_string().contains("Rust"));
        assert!(error.to_string().contains("include"));
        assert!(error.to_string().contains("extend_include"));
    }

    #[test]
    fn it_rejects_empty_override_path_lists() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [[overrides]]
                paths = []
                reason = "temporary exception"
            "#},
        )
        .unwrap();

        let error = load_config_fixture(tempdir.path()).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("override paths must not be empty")
        );
    }

    #[test]
    fn it_rejects_override_entries_without_limits() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [[overrides]]
                paths = ["generated/**"]
                reason = "temporary exception"
            "#},
        )
        .unwrap();

        let error = load_config_fixture(tempdir.path()).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("override must set at least one limit")
        );
    }

    #[test]
    fn it_rejects_override_entries_with_empty_limit_tables() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [[overrides]]
                paths = ["generated/**"]
                max_direct_files_per_directory = {}
                reason = "generated sources"
            "#},
        )
        .unwrap();

        let error = load_config_fixture(tempdir.path()).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("override must set at least one limit")
        );
    }

    #[test]
    fn it_rejects_override_entries_without_reasons() {
        assert_config_error_contains(
            indoc! {r#"
                [[overrides]]
                paths = ["generated/**"]
                max_lines_per_file = { production = 900 }
            "#},
            "missing field `reason`",
        );
    }

    #[test]
    fn it_rejects_blank_override_reasons() {
        assert_config_error_contains(
            indoc! {r#"
                [[overrides]]
                paths = ["generated/**"]
                max_lines_per_file = { production = 900 }
                reason = "   "
            "#},
            "override reason must not be empty",
        );
    }

    #[test]
    fn it_rejects_count_lines_in_path_overrides() {
        assert_config_error_contains(
            indoc! {r#"
                [[overrides]]
                paths = ["generated/**"]
                max_lines_per_file = { count_lines = [] }
                reason = "generated sources"
            "#},
            "unknown field `count_lines`",
        );
    }

    #[test]
    fn it_rejects_language_and_group_selectors_in_path_overrides() {
        for selector in ["language = \"rust\"", "language_group = \"source\""] {
            assert_config_error_contains(
                &format!(
                    "[[overrides]]\npaths = [\"generated/**\"]\n{selector}\nmax_lines_per_file = {{ production = 900 }}\nreason = \"generated sources\"\n"
                ),
                "unknown field",
            );
        }
    }

    #[test]
    fn it_rejects_invalid_language_group_definitions() {
        let cases = [
            (
                "[language_groups.frontend]\nlanguages = []\n",
                "language group must contain at least one language",
            ),
            (
                "[language_groups.frontend]\nlanguages = [\"rust\", \"rust\"]\n",
                "language group contains duplicate language Rust",
            ),
            (
                "[language_groups.frontend]\nlanguages = [\"not-a-language\"]\n",
                "unsupported language ID",
            ),
            (
                "[language_groups.Frontend]\nlanguages = [\"rust\"]\n",
                "must start with an ASCII lowercase letter",
            ),
            (
                "[language_groups.source]\nlanguages = [\"rust\"]\n",
                "source group is configured through global limits",
            ),
            (
                "[language_groups.documentation]\nlanguages = [\"markdown\"]\n",
                "documentation group membership is automatic",
            ),
            (
                "[language_groups.frontend]\n",
                "custom language group frontend must declare languages",
            ),
            (
                "[language_groups.frontend]\nlanguages = [\"rust\"]\nunknown = true\n",
                "unknown field `unknown`",
            ),
            (
                "[language_groups.frontend]\nlanguages = [\"rust\"]\n[language_groups.frontend.limits.max_direct_files_per_directory]\nproduction = 10\nunknown = 20\n",
                "unknown field `unknown`",
            ),
        ];

        for (config, expected) in cases {
            assert_config_error_contains(config, expected);
        }
    }

    #[test]
    fn it_rejects_languages_assigned_to_multiple_custom_groups() {
        assert_config_error_contains(
            indoc! {r#"
                [language_groups.frontend]
                languages = ["javascript", "typescript"]

                [language_groups.browser]
                languages = ["typescript", "jsx"]
            "#},
            "language TypeScript belongs to both browser and frontend",
        );
    }
}
