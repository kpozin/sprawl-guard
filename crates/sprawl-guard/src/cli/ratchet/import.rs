use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use sprawl_guard_lib::{CheckViolation, RelativePath, SprawlError};

use crate::cli::linter::{self, ImportError, Invocation, Linter, MAX_LINES, OXLINT_MAX_LINES};
use crate::error::Result;

/// Restricts ratchet additions to files with existing linter suppressions.
pub(super) struct Suppressions(BTreeSet<RelativePath>);

impl Suppressions {
    pub(super) fn load(root: &Path, violations: &[CheckViolation], linter: Linter) -> Result<Self> {
        let file = root.join(match linter {
            Linter::Eslint => "eslint-suppressions.json",
            Linter::Oxlint => "oxlint-suppressions.json",
        });
        let mut paths = read_bulk_suppressions(&file)?;
        let languages = linter::js_languages();
        let candidates = violations
            .iter()
            .filter_map(|violation| match violation {
                CheckViolation::FileLines(file)
                    if languages.contains(&file.language) && !paths.contains(&file.path) =>
                {
                    Some(&file.path)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        match linter {
            Linter::Eslint => import_eslint(root, &candidates, &mut paths)?,
            Linter::Oxlint => import_oxlint(root, &candidates, &mut paths)?,
        }

        Ok(Self(paths))
    }

    pub(super) fn covers(&self, path: &RelativePath) -> bool {
        self.0.contains(path)
    }
}

/// Leaves headroom under Windows' command-line limit when batching typical source paths.
const ESLINT_FILES_PER_BATCH: usize = 64;

fn import_eslint(
    root: &Path,
    candidates: &[&RelativePath],
    suppressed: &mut BTreeSet<RelativePath>,
) -> Result<()> {
    let paths = candidates
        .iter()
        .map(|&path| {
            // Discovery can follow symlinks when the project enables that option.
            under_root(root, &canonicalize(&root.join(path.to_path_buf()))?)?;
            Ok(path.clone())
        })
        .collect::<Result<Vec<_>>>()?;
    for batch in paths.chunks(ESLINT_FILES_PER_BATCH) {
        let [first, remaining @ ..] = batch else {
            continue;
        };
        let results: Vec<EslintResult> =
            linter::run(root, Invocation::EslintReport { first, remaining })?;
        for result in results {
            if result.fatal_error_count > 0 {
                return Err(ImportError::EslintFatal {
                    path: result.file_path,
                }
                .into());
            }
            if result
                .suppressed_messages
                .iter()
                .any(|message| message.rule_id.as_deref() == Some(MAX_LINES))
            {
                let path = linter::reported_path(result.file_path)?;
                suppressed.insert(under_root(root, &path)?);
            }
        }
    }
    Ok(())
}

fn import_oxlint(
    root: &Path,
    candidates: &[&RelativePath],
    suppressed: &mut BTreeSet<RelativePath>,
) -> Result<()> {
    let mut eslint_aliases = Vec::new();
    for &candidate in candidates {
        let path = root.join(candidate.to_path_buf());
        let source =
            fs::read_to_string(&path).map_err(|source| ImportError::Read { path, source })?;
        match oxlint_file_disable(&source) {
            Some(OxlintDisable::Native) => {
                suppressed.insert(candidate.clone());
            }
            Some(OxlintDisable::EslintAlias) => eslint_aliases.push(candidate),
            None => {}
        }
    }
    if !eslint_aliases.is_empty() && respects_eslint_directives(root)? {
        suppressed.extend(eslint_aliases.into_iter().cloned());
    }
    Ok(())
}

#[derive(Deserialize)]
struct OxlintConfig {
    #[serde(default)]
    options: OxlintOptions,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OxlintOptions {
    respect_eslint_disable_directives: Option<bool>,
}

/// ESLint aliases need Oxlint's resolved configuration, while native directives work without Oxlint.
fn respects_eslint_directives(root: &Path) -> Result<bool> {
    let options = match linter::run::<OxlintConfig>(root, Invocation::OxlintConfig) {
        Ok(config) => config.options,
        Err(ImportError::MissingPackage { .. }) => {
            return Ok(false);
        }
        Err(ImportError::Launch { source, .. }) if source.kind() == ErrorKind::NotFound => {
            return Ok(false);
        }
        Err(error) => return Err(error.into()),
    };
    Ok(options.respect_eslint_disable_directives.unwrap_or(true))
}

fn canonicalize(path: &Path) -> Result<PathBuf> {
    path.canonicalize().map_err(|source| {
        ImportError::Read {
            path: path.to_path_buf(),
            source,
        }
        .into()
    })
}

fn under_root(root: &Path, path: &Path) -> Result<RelativePath> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| SprawlError::PathOutsideRoot {
            path: path.to_path_buf(),
            root: root.to_path_buf(),
        })?;
    Ok(RelativePath::new(relative)?)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EslintResult {
    file_path: PathBuf,
    #[serde(default)]
    suppressed_messages: Vec<EslintMessage>,
    #[serde(default)]
    fatal_error_count: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EslintMessage {
    rule_id: Option<String>,
}

#[derive(Deserialize)]
struct BulkCount {
    count: u32,
}

fn read_bulk_suppressions(path: &Path) -> Result<BTreeSet<RelativePath>> {
    let data = match fs::read(path) {
        Ok(data) => data,
        Err(source) if source.kind() == ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(source) => {
            return Err(ImportError::Read {
                path: path.to_path_buf(),
                source,
            }
            .into());
        }
    };
    let report: BTreeMap<PathBuf, BTreeMap<String, BulkCount>> = serde_json::from_slice(&data)
        .map_err(|source| ImportError::Suppressions {
            path: path.to_path_buf(),
            source,
        })?;
    report
        .into_iter()
        .filter(|(_, rules)| {
            [MAX_LINES, OXLINT_MAX_LINES, "eslint(max-lines)"]
                .iter()
                .any(|rule| rules.get(*rule).is_some_and(|entry| entry.count > 0))
        })
        .map(|(file, _)| Ok(RelativePath::new(file)?))
        .collect()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OxlintDisable {
    Native,
    EslintAlias,
}

/// Distinguishes native disables from aliases that depend on Oxlint's resolved configuration.
fn oxlint_file_disable(source: &str) -> Option<OxlintDisable> {
    // Treat enable markers as ambiguous without parsing the whole file.
    if source.contains("oxlint-enable") || source.contains("eslint-enable") {
        return None;
    }
    let mut disable = None;
    let mut rest = source.trim_start_matches('\u{feff}').trim_start();
    while let Some((comment, tail)) = leading_comment(rest) {
        let (directive, _) = comment.split_once(" -- ").unwrap_or((comment, ""));
        let directive = directive.trim();
        let (keyword, rules) = directive
            .split_once(char::is_whitespace)
            .unwrap_or((directive, ""));
        if rules.trim().is_empty()
            || rules
                .split(',')
                .any(|rule| matches!(rule.trim(), MAX_LINES | OXLINT_MAX_LINES))
        {
            match keyword {
                "oxlint-disable" => return Some(OxlintDisable::Native),
                "eslint-disable" => disable = Some(OxlintDisable::EslintAlias),
                _ => {}
            }
        }
        rest = tail.trim_start();
    }
    disable
}

fn leading_comment(source: &str) -> Option<(&str, &str)> {
    if let Some(comment) = source.strip_prefix("/*") {
        return comment.split_once("*/");
    }
    let comment = source.strip_prefix("//")?;
    Some(comment.split_once(['\r', '\n']).unwrap_or((comment, "")))
}

#[cfg(test)]
mod tests {
    use super::*;

    mod when_reading_an_oxlint_header {
        use super::*;

        mod and_the_header_uses_eslint_disable {
            use super::*;

            #[test]
            fn it_identifies_an_eslint_alias() {
                assert_eq!(
                    oxlint_file_disable("/* eslint-disable no-console, max-lines -- legacy */"),
                    Some(OxlintDisable::EslintAlias),
                );
            }
        }

        mod and_the_header_disables_all_rules {
            use super::*;

            #[test]
            fn it_identifies_the_native_directive() {
                assert_eq!(
                    oxlint_file_disable("/* oxlint-disable -- generated */"),
                    Some(OxlintDisable::Native),
                );
            }
        }

        mod and_a_line_comment_follows_a_bom {
            use super::*;

            #[test]
            fn it_identifies_the_native_directive() {
                assert_eq!(
                    oxlint_file_disable("\u{feff}// oxlint-disable eslint/max-lines\r\ncode"),
                    Some(OxlintDisable::Native),
                );
            }
        }

        mod and_the_directive_cannot_disable_max_lines {
            use super::*;

            #[test]
            fn it_returns_no_disable() {
                for source in [
                    "/* oxlint-disable no-debugger */",
                    "/* oxlint-disable max - lines */",
                    "/* oxlint-disable-next-line max-lines */",
                    "/* oxlint-disable max-lines",
                    "`/* oxlint-disable max-lines */`",
                    "/* oxlint-disable max-lines */\n/* eslint-enable max-lines */",
                ] {
                    assert_eq!(oxlint_file_disable(source), None, "{source:?}");
                }
            }
        }
    }

    mod when_reading_a_leading_comment {
        use super::*;

        #[test]
        fn it_leaves_the_source_after_the_first_comment() {
            assert_eq!(
                leading_comment("/* first */code"),
                Some((" first ", "code"))
            );
            assert_eq!(leading_comment("// first\ncode"), Some((" first", "code")));
            assert_eq!(leading_comment("/* unfinished"), None);
        }
    }
}
