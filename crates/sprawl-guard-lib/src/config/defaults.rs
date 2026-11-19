use crate::config::{
    CommonLanguageConfig, Config, ConfigSchemaVersion, FilesConfig, IgnoreConfig, LanguageConfig,
    LanguageGroupsConfig, LanguageLimits, LanguageMode, Limits, MaxDirectFilesPerDirectoryLimits,
    MaxLinesPerFileLimits, OutputConfig, OutputFormat, ParsedCodeFileByteLimit, RatchetConfig,
    RustCfgTestExclusion, RustLanguageConfig, SymlinkTraversal, TestsConfig, TraversalConfig,
    TraversalEntryLimit, UnreadablePathPolicy,
};
use crate::{
    CountLines, DirectFileLimit, DirectFiles, FiniteOrUnlimited, GlobPattern, LanguageId,
    LineLimit, MaxBytesPerFile, MaxFindings, RatchetFilePath,
};
use tokei::LanguageType;

const DEFAULT_MAX_NON_TEST_CODE_LINES_PER_FILE: u32 = 400;
const DEFAULT_MAX_DIRECT_FILES_PER_DIRECTORY: u32 = 10;
const DEFAULT_MAX_CODE_LINES_PER_TEST_FILE: u32 = 800;
const DEFAULT_MAX_TRAVERSAL_ENTRIES: u32 = 1_000_000;
const DEFAULT_MAX_PARSED_CODE_FILE_BYTES: ParsedCodeFileByteLimit =
    match ParsedCodeFileByteLimit::try_new(10 * 1024 * 1024) {
        Ok(limit) => limit,
        Err(_) => panic!("the default parsed-code file byte limit is positive"),
    };
/// Enough findings to show the shape of a problem while keeping the summary on screen.
const DEFAULT_MAX_FINDINGS: u32 = 50;
// GitHub's code-review exclusions provide the baseline `*.lock` and `go.sum` excludes:
// https://docs.github.com/en/copilot/reference/review-excluded-files
// Exact names cover common lockfiles whose names do not end in `.lock`.
const DEFAULT_DEPENDENCY_LOCKFILE_EXCLUDES: [&str; 7] = [
    "**/*.lock",
    "**/bun.lockb",
    "**/go.sum",
    "**/npm-shrinkwrap.json",
    "**/package-lock.json",
    "**/Package.resolved",
    "**/pnpm-lock.yaml",
];

/// Returns the built-in resolved configuration.
pub fn built_in_config() -> Config {
    let languages = default_languages();
    let language_groups = LanguageGroupsConfig::automatic(languages.keys().copied());
    Config {
        authored_file: None,
        schema_version: ConfigSchemaVersion::CURRENT,
        limits: Limits {
            max_lines_per_file: MaxLinesPerFileLimits {
                count_lines: CountLines::Code,
                production: LineLimit::trusted(DEFAULT_MAX_NON_TEST_CODE_LINES_PER_FILE),
                test: LineLimit::trusted(DEFAULT_MAX_CODE_LINES_PER_TEST_FILE),
            },
            max_direct_files_per_directory: MaxDirectFilesPerDirectoryLimits {
                production: DirectFileLimit::Finite(DirectFiles::trusted(
                    DEFAULT_MAX_DIRECT_FILES_PER_DIRECTORY,
                )),
                test: DirectFileLimit::Finite(DirectFiles::trusted(
                    DEFAULT_MAX_DIRECT_FILES_PER_DIRECTORY,
                )),
            },
            max_bytes_per_file: MaxBytesPerFile::Unlimited,
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
            max_entries: TraversalEntryLimit::trusted(DEFAULT_MAX_TRAVERSAL_ENTRIES),
            max_parsed_code_file_bytes: DEFAULT_MAX_PARSED_CODE_FILE_BYTES,
            unreadable_paths: UnreadablePathPolicy::Fail,
        },
        files: FilesConfig {
            extend_exclude: glob_patterns(DEFAULT_DEPENDENCY_LOCKFILE_EXCLUDES),
        },
        tests: TestsConfig {
            check: true,
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
            max_findings: FiniteOrUnlimited::Finite(MaxFindings::trusted(DEFAULT_MAX_FINDINGS)),
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
            extend_include: vec![],
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
        extend_include: vec![],
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
