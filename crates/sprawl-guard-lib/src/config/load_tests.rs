#![cfg(test)]

use std::fs;
use std::path::Path;

use indoc::indoc;
use tokei::LanguageType;

use super::*;
use crate::config::{
    SourceFileByteLimit, SymlinkTraversal, TestFanoutLimit, TraversalDepth, TraversalEntryLimit,
    TraversalErrorPolicy,
};
use crate::{GlobPattern, LeafFiles};

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
        let rust = loaded
            .config
            .languages
            .get(&LanguageId::new("rust").unwrap())
            .unwrap();

        assert!(rust.is_enabled());
        assert_patterns(&rust.common().include, &["**/*.rs"]);
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
        }
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
                [limits]
                max_lines_per_file = { code_non_test = 500, code = 900 }
            "#},
        )
        .unwrap();

        let loaded = load(tempdir.path()).unwrap();

        assert_eq!(
            loaded.config.limits.max_lines_per_file.code_non_test.get(),
            500
        );
        assert_eq!(loaded.config.limits.max_lines_per_file.code.get(), 900);
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
            loaded
                .config
                .limits
                .max_leaf_files_per_directory
                .production
                .get(),
            10
        );
        assert_eq!(
            loaded.config.limits.max_leaf_files_per_directory.test,
            TestFanoutLimit::Finite(LeafFiles::new(20).unwrap())
        );
    }

    #[test]
    fn it_accepts_unlimited_test_fanout() {
        let tempdir = tempfile::tempdir().unwrap();
        fs::write(
            tempdir.path().join(PRIMARY_CONFIG_FILE),
            indoc! {r#"
                [limits]
                max_leaf_files_per_directory = { test = "unlimited" }
            "#},
        )
        .unwrap();

        let loaded = load(tempdir.path()).unwrap();

        assert!(
            loaded
                .config
                .limits
                .max_leaf_files_per_directory
                .test
                .is_unlimited()
        );
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
                code_non_test = 0
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
                code_non_test = 900
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
                max_lines_per_file = { code_non_test = 900, code = 1200 }
                reason = "b"
            "#},
        )
        .unwrap();
        fs::write(
            tempdir.path().join("sprawl-guard.d/a.toml"),
            indoc! {r#"
                [[overrides]]
                paths = ["a/**"]
                max_lines_per_file = { code_non_test = 800 }
                reason = "a"
            "#},
        )
        .unwrap();

        let loaded = load(tempdir.path()).unwrap();
        let reasons = loaded
            .config
            .overrides
            .iter()
            .map(|override_config| override_config.reason.as_ref().unwrap().as_str())
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
