#![cfg(test)]

use std::fs;
use std::path::Path;

use indoc::indoc;
use tokei::LanguageType;

use super::*;
use crate::config::{
    LanguageConfig, SourceFileByteLimit, SymlinkTraversal, TraversalDepth, TraversalEntryLimit,
    TraversalErrorPolicy,
};
use crate::{
    CustomLanguageGroupId, GlobPattern, IncludedLineTypes, LanguageGroupId, LeafFileLimit,
    LeafFiles,
};

fn load(root: &Path) -> Result<LoadedConfig> {
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

mod when_config_is_minimal {
    use super::*;

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

        let loaded = load(tempdir.path()).unwrap();
        let rust = language(&loaded.config, "rust");

        assert!(rust.is_enabled());
        assert_eq!(rust.common().include, None);
        assert!(rust.common().additional_globs.is_empty());
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

        let loaded = load(tempdir.path()).unwrap();

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
        let loaded = load(tempdir.path()).unwrap();

        let serialized = toml::Value::try_from(&loaded.config).unwrap();
        let rust = serialized
            .get("languages")
            .and_then(|languages| languages.get("Rust"))
            .and_then(toml::Value::as_table)
            .unwrap();

        assert!(!rust.contains_key("include"));
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

        let loaded = load(tempdir.path()).unwrap();

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

        let loaded = load(tempdir.path()).unwrap();
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
                additional_globs = ["generated/**/*.rust"]
            "#},
        )
        .unwrap();

        let loaded = load(tempdir.path()).unwrap();
        let rust = language(&loaded.config, "rust").common();

        assert_eq!(rust.include, None);
        assert_patterns(&rust.additional_globs, &["generated/**/*.rust"]);
    }

    #[test]
    fn it_merges_all_language_file_limit_fields() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [languages.rust.limits.max_lines_per_file]
                included_line_types = ["code", "comments"]
                production = 500
                test = 1000
            "#},
        )
        .unwrap();

        let loaded = load(tempdir.path()).unwrap();
        let limits = &language(&loaded.config, "rust")
            .common()
            .limits
            .max_lines_per_file;

        assert_eq!(
            limits.included_line_types,
            Some(IncludedLineTypes::CodeAndComments)
        );
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

        let loaded = load(tempdir.path()).unwrap();

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

        let loaded = load(tempdir.path()).unwrap();

        assert!(!language(&loaded.config, "rust").is_enabled());
    }
}

mod when_config_has_unknown_keys {
    use super::*;

    #[test]
    fn it_rejects_the_config() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(tempdir.path().join(PRIMARY_CONFIG_FILE), "unknown = true\n").unwrap();

        let error = load(tempdir.path()).unwrap_err();

        assert!(error.to_string().contains("unknown field"));
    }
}

mod when_config_include_table_is_sparse {
    use super::*;

    #[test]
    fn it_keeps_default_include_settings() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(tempdir.path().join(PRIMARY_CONFIG_FILE), "[config]\n").unwrap();

        let loaded = load(tempdir.path()).unwrap();

        assert!(loaded.config.config.include.is_empty());
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

        let loaded = load(tempdir.path()).unwrap();

        assert_patterns(
            &loaded.config.tests.globs,
            &["custom-tests/**", "**/*.fixture.*"],
        );
        assert!(loaded.config.tests.check_path_tests);
    }
}

mod when_traversal_config_is_partial {
    use super::*;

    #[test]
    fn it_merges_symlink_and_depth_policy() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [traversal]
                follow_symlinks = "files-and-directories"
                same_file_system = false
                max_depth = 32
                max_entries = 1234
                max_file_bytes = 4096
                error_policy = "warn"
            "#},
        )
        .unwrap();

        let loaded = load(tempdir.path()).unwrap();

        assert_eq!(
            loaded.config.traversal.follow_symlinks,
            SymlinkTraversal::FilesAndDirectories
        );
        assert!(!loaded.config.traversal.same_file_system);
        assert_eq!(
            loaded.config.traversal.max_depth,
            TraversalDepth::new(32).unwrap()
        );
        assert_eq!(
            loaded.config.traversal.max_entries,
            TraversalEntryLimit::new(1234).unwrap()
        );
        assert_eq!(
            loaded.config.traversal.max_file_bytes,
            SourceFileByteLimit::new(4096).unwrap()
        );
        assert_eq!(
            loaded.config.traversal.error_policy,
            TraversalErrorPolicy::Warn
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
                included_line_types = ["blank_lines", "code", "comments"]
                production = 500
                test = 900
            "#},
        )
        .unwrap();

        let loaded = load(tempdir.path()).unwrap();

        assert_eq!(
            loaded.config.limits.max_lines_per_file.production.get(),
            500
        );
        assert_eq!(loaded.config.limits.max_lines_per_file.test.get(), 900);
        assert_eq!(
            loaded.config.limits.max_lines_per_file.included_line_types,
            IncludedLineTypes::All
        );
    }

    #[test]
    fn it_merges_directory_fanout_limits() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [limits]
                max_leaf_files_per_directory = { production = 10, test = 20 }
            "#},
        )
        .unwrap();

        let loaded = load(tempdir.path()).unwrap();

        assert_eq!(
            loaded.config.limits.max_leaf_files_per_directory.production,
            LeafFileLimit::Finite(LeafFiles::new(10).unwrap())
        );
        assert_eq!(
            loaded.config.limits.max_leaf_files_per_directory.test,
            LeafFileLimit::Finite(LeafFiles::new(20).unwrap())
        );
    }

    #[test]
    fn it_accepts_unlimited_fanout_for_both_roles() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [limits]
                max_leaf_files_per_directory = { production = "unlimited", test = "unlimited" }
            "#},
        )
        .unwrap();

        let loaded = load(tempdir.path()).unwrap();

        assert_eq!(
            loaded.config.limits.max_leaf_files_per_directory.production,
            LeafFileLimit::Unlimited
        );
        assert_eq!(
            loaded.config.limits.max_leaf_files_per_directory.test,
            LeafFileLimit::Unlimited
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
                max_leaf_files_per_directory = { production = 20, test = "unlimited" }
                reason = "The web package colocates route modules"
            "#},
        )
        .unwrap();

        let loaded = load(tempdir.path()).unwrap();
        let limits = &loaded.config.overrides[0].limits;

        assert_eq!(limits.production_loc_per_file().unwrap().get(), 600);
        assert_eq!(limits.test_loc_per_file().unwrap().get(), 1200);
        assert_eq!(
            limits.production_leaf_files_per_directory(),
            Some(LeafFileLimit::Finite(LeafFiles::new(20).unwrap()))
        );
        assert_eq!(
            limits.test_leaf_files_per_directory(),
            Some(LeafFileLimit::Unlimited)
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
                [limits.max_leaf_files_per_directory]
                production = 10
                test = 20

                [language_groups.frontend]
                languages = ["javascript", "typescript", "jsx", "tsx"]

                [language_groups.frontend.limits.max_leaf_files_per_directory]
                production = 12
            "#},
        )
        .unwrap();

        let loaded = load(tempdir.path()).unwrap();
        let frontend = CustomLanguageGroupId::try_new("frontend".to_owned()).unwrap();
        let group = loaded
            .config
            .language_groups()
            .custom()
            .get(&frontend)
            .unwrap();

        assert_eq!(
            group.limits.max_leaf_files_per_directory,
            crate::config::MaxLeafFilesPerDirectoryLimits {
                production: LeafFileLimit::Finite(LeafFiles::new(12).unwrap()),
                test: LeafFileLimit::Finite(LeafFiles::new(20).unwrap()),
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

        let loaded = load(tempdir.path()).unwrap();

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
                [language_groups.documentation.limits.max_leaf_files_per_directory]
                production = 20
            "#},
        )
        .unwrap();

        let loaded = load(tempdir.path()).unwrap();

        assert_eq!(
            loaded
                .config
                .language_groups()
                .documentation()
                .limits
                .max_leaf_files_per_directory,
            crate::config::MaxLeafFilesPerDirectoryLimits {
                production: LeafFileLimit::Finite(LeafFiles::new(20).unwrap()),
                test: LeafFileLimit::Unlimited,
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
        let loaded = load(tempdir.path()).unwrap();

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

        let error = load(tempdir.path()).unwrap_err();

        assert!(error.to_string().contains("failed to parse"));
    }
}

mod when_config_values_are_invalid {
    use super::*;

    fn assert_config_error_contains(config: &str, expected: &str) {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(tempdir.path().join(PRIMARY_CONFIG_FILE), config).unwrap();

        let error = load(tempdir.path()).unwrap_err();

        assert!(error.to_string().contains(expected));
    }

    #[test]
    fn it_rejects_invalid_globs() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [files]
                exclude = ["["]
            "#},
        )
        .unwrap();

        let error = load(tempdir.path()).unwrap_err();

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

        let error = load(tempdir.path()).unwrap_err();

        assert!(error.to_string().contains("must be greater than zero"));
    }

    #[test]
    fn it_rejects_zero_traversal_depth() {
        assert_config_error_contains(
            indoc! {r#"
                [traversal]
                max_depth = 0
            "#},
            "traversal max_depth must be greater than zero",
        );
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
    fn it_rejects_unknown_test_fanout_keywords() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [limits]
                max_leaf_files_per_directory = { test = "many" }
            "#},
        )
        .unwrap();

        let error = load(tempdir.path()).unwrap_err();

        assert!(error.to_string().contains("expected a positive integer"));
    }

    #[test]
    fn it_rejects_missing_code_in_included_line_types() {
        assert_config_error_contains(
            indoc! {r#"
                [limits.max_lines_per_file]
                included_line_types = ["comments"]
            "#},
            "included_line_types must include code",
        );
    }

    #[test]
    fn it_rejects_duplicate_included_line_types() {
        assert_config_error_contains(
            indoc! {r#"
                [limits.max_lines_per_file]
                included_line_types = ["code", "comments", "code"]
            "#},
            "included_line_types contains duplicate code",
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

        let error = load(tempdir.path()).unwrap_err();

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

        let error = load(tempdir.path()).unwrap_err();

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

        let error = load(tempdir.path()).unwrap_err();

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

        let error = load(tempdir.path()).unwrap_err();

        assert!(error.to_string().contains("must not contain ':'"));
    }

    #[test]
    fn it_rejects_unknown_color_modes() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [output]
                color = "sometimes"
            "#},
        )
        .unwrap();

        let error = load(tempdir.path()).unwrap_err();

        assert!(error.to_string().contains("unsupported color mode"));
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

        let error = load(tempdir.path()).unwrap_err();

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

        let error = load(tempdir.path()).unwrap_err();

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

        let error = load(tempdir.path()).unwrap_err();

        assert!(error.to_string().contains("Rust"));
        assert!(error.to_string().contains("include"));
        assert!(error.to_string().contains("additional_globs"));
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

        let error = load(tempdir.path()).unwrap_err();

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

        let error = load(tempdir.path()).unwrap_err();

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
                max_leaf_files_per_directory = {}
                reason = "generated sources"
            "#},
        )
        .unwrap();

        let error = load(tempdir.path()).unwrap_err();

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
    fn it_rejects_included_line_types_in_path_overrides() {
        assert_config_error_contains(
            indoc! {r#"
                [[overrides]]
                paths = ["generated/**"]
                max_lines_per_file = { included_line_types = ["code"] }
                reason = "generated sources"
            "#},
            "unknown field `included_line_types`",
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
                "[language_groups.frontend]\nlanguages = [\"rust\"]\n[language_groups.frontend.limits.max_leaf_files_per_directory]\nproduction = 10\nunknown = 20\n",
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

mod when_both_config_names_exist {
    use super::*;

    #[test]
    fn it_reports_ambiguity() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(tempdir.path().join(PRIMARY_CONFIG_FILE), "").unwrap();
        fs::write(tempdir.path().join(ALIAS_CONFIG_FILE), "").unwrap();

        let error = load(tempdir.path()).unwrap_err();

        assert!(error.to_string().contains("ambiguous config files"));
    }
}

mod when_include_fragment_contains_non_override_tables {
    use super::*;

    #[test]
    fn it_rejects_the_fragment() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::create_dir(tempdir.path().join("sprawl-guard.d")).unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [config]
                include = ["sprawl-guard.d/*.toml"]
            "#},
        )
        .unwrap();
        fs::write(
            tempdir.path().join("sprawl-guard.d/bad.toml"),
            indoc! {r#"
                [limits.max_lines_per_file]
                production = 900
            "#},
        )
        .unwrap();

        let error = load(tempdir.path()).unwrap_err();

        assert!(error.to_string().contains("may contain only [[overrides]]"));
    }
}

mod when_include_fragments_match {
    use super::*;

    #[test]
    fn it_appends_overrides_in_lexicographic_order() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::create_dir(tempdir.path().join("sprawl-guard.d")).unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [config]
                include = ["sprawl-guard.d/*.toml"]

                [[overrides]]
                paths = ["root/**"]
                max_leaf_files_per_directory = { production = 7, test = "unlimited" }
                reason = "root"
            "#},
        )
        .unwrap();
        fs::write(
            tempdir.path().join("sprawl-guard.d/b.toml"),
            indoc! {r#"
                [[overrides]]
                paths = ["b/**"]
                max_lines_per_file = { production = 900, test = 1200 }
                reason = "b"
            "#},
        )
        .unwrap();
        fs::write(
            tempdir.path().join("sprawl-guard.d/a.toml"),
            indoc! {r#"
                [[overrides]]
                paths = ["a/**"]
                max_lines_per_file = { production = 800 }
                reason = "a"
            "#},
        )
        .unwrap();

        let loaded = load(tempdir.path()).unwrap();
        let reasons = loaded
            .config
            .overrides
            .iter()
            .map(|override_config| override_config.reason.as_str())
            .collect::<Vec<_>>();

        assert_eq!(reasons, vec!["root", "a", "b"]);
    }
}

#[cfg(unix)]
mod when_include_globs_match_symlink_directories {
    use std::os::unix::fs::symlink;

    use super::*;

    #[test]
    fn it_does_not_descend_into_the_symlink() {
        let tempdir = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        fs::create_dir(tempdir.path().join("sprawl-guard.d")).unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [config]
                include = ["sprawl-guard.d/**/*.toml"]
            "#},
        )
        .unwrap();
        fs::write(
            external.path().join("external.toml"),
            indoc! {r#"
                [[overrides]]
                paths = ["external/**"]
                reason = "external"
            "#},
        )
        .unwrap();
        symlink(
            external.path(),
            tempdir.path().join("sprawl-guard.d/external"),
        )
        .unwrap();

        let loaded = load(tempdir.path()).unwrap();

        assert!(loaded.includes.is_empty());
        assert!(loaded.config.overrides.is_empty());
    }
}
