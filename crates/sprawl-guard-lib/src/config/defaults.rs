use crate::config::{
    CommonLanguageConfig, Config, ConfigInclude, FilesConfig, IgnoreConfig, LanguageConfig,
    LanguageGroupsConfig, LanguageLimits, LanguageMode, Limits, MaxLeafFilesPerDirectoryLimits,
    MaxLinesPerFileLimits, OutputConfig, OutputFormat, RatchetConfig, RustCfgTestExclusion,
    RustLanguageConfig, SourceFileByteLimit, SymlinkTraversal, TestsConfig, TraversalConfig,
    TraversalDepth, TraversalEntryLimit, TraversalErrorPolicy,
};
use crate::{
    ColorMode, GlobPattern, IncludedLineTypes, LanguageId, LeafFileLimit, LeafFiles, LocLimit,
    RatchetFilePath,
};
use tokei::LanguageType;

const DEFAULT_MAX_NON_TEST_CODE_LINES_PER_FILE: u32 = 400;
const DEFAULT_MAX_LEAF_SOURCE_FILES_PER_DIRECTORY: u32 = 8;
const DEFAULT_MAX_CODE_LINES_PER_TEST_FILE: u32 = 800;
const DEFAULT_MAX_TRAVERSAL_DEPTH: u32 = 128;
const DEFAULT_MAX_TRAVERSAL_ENTRIES: u32 = 1_000_000;
const DEFAULT_MAX_SOURCE_FILE_BYTES: u64 = 10 * 1024 * 1024;

/// Returns the built-in resolved configuration.
pub fn built_in_config() -> Config {
    let languages = default_languages();
    let language_groups = LanguageGroupsConfig::automatic(languages.keys().copied());
    Config {
        config: ConfigInclude { include: vec![] },
        limits: Limits {
            max_lines_per_file: MaxLinesPerFileLimits {
                included_line_types: IncludedLineTypes::Code,
                production: LocLimit::trusted(DEFAULT_MAX_NON_TEST_CODE_LINES_PER_FILE),
                test: LocLimit::trusted(DEFAULT_MAX_CODE_LINES_PER_TEST_FILE),
            },
            max_leaf_files_per_directory: MaxLeafFilesPerDirectoryLimits {
                production: LeafFileLimit::Finite(LeafFiles::trusted(
                    DEFAULT_MAX_LEAF_SOURCE_FILES_PER_DIRECTORY,
                )),
                test: LeafFileLimit::Unlimited,
            },
        },
        ignore: IgnoreConfig {
            respect_gitignore: true,
            respect_ignore: true,
            respect_rgignore: true,
            respect_fdignore: true,
            skip_hidden: true,
        },
        traversal: TraversalConfig {
            follow_symlinks: SymlinkTraversal::None,
            same_file_system: true,
            max_depth: TraversalDepth::trusted(DEFAULT_MAX_TRAVERSAL_DEPTH),
            max_entries: TraversalEntryLimit::trusted(DEFAULT_MAX_TRAVERSAL_ENTRIES),
            max_file_bytes: SourceFileByteLimit::trusted(DEFAULT_MAX_SOURCE_FILE_BYTES),
            error_policy: TraversalErrorPolicy::Fail,
        },
        files: FilesConfig { exclude: vec![] },
        tests: TestsConfig {
            check_path_tests: true,
            globs: glob_patterns([
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
            ]),
        },
        languages,
        language_groups,
        overrides: vec![],
        ratchet: RatchetConfig {
            file: RatchetFilePath::trusted("sprawl-guard.ratchet.toml"),
            require_current: true,
        },
        output: OutputConfig {
            format: OutputFormat::Human,
            color: ColorMode::Auto,
        },
    }
}

/// Returns every language ID supported by the bundled `tokei` language table.
pub fn supported_language_ids() -> Vec<LanguageId> {
    LanguageType::list()
        .iter()
        .map(|(language, _)| LanguageId::from_tokei(*language))
        .collect()
}

fn default_languages() -> std::collections::BTreeMap<LanguageId, LanguageConfig> {
    LanguageType::list()
        .iter()
        .map(|(language, _)| {
            (
                LanguageId::from_tokei(*language),
                default_language_config(*language),
            )
        })
        .collect()
}

fn default_language_config(language: LanguageType) -> LanguageConfig {
    match language {
        LanguageType::Rust => rust_language_config(),
        _ => generic_language_config(),
    }
}

fn rust_language_config() -> LanguageConfig {
    LanguageConfig::rust_config(
        CommonLanguageConfig {
            mode: LanguageMode::Enabled,
            include: None,
            additional_globs: vec![],
            exclude: vec![],
            test_globs: glob_patterns(["benches/**"]),
            limits: LanguageLimits::default(),
        },
        RustLanguageConfig {
            cfg_test_exclusion: RustCfgTestExclusion::Enabled,
        },
    )
}

fn generic_language_config() -> LanguageConfig {
    LanguageConfig::generic(CommonLanguageConfig {
        mode: LanguageMode::Enabled,
        include: None,
        additional_globs: vec![],
        exclude: vec![],
        test_globs: vec![],
        limits: LanguageLimits::default(),
    })
}

fn glob_patterns<const N: usize>(patterns: [&str; N]) -> Vec<GlobPattern> {
    patterns.into_iter().map(glob_pattern).collect()
}

fn glob_pattern(pattern: impl Into<String>) -> GlobPattern {
    GlobPattern::new(pattern).expect("built-in glob pattern must be valid")
}
