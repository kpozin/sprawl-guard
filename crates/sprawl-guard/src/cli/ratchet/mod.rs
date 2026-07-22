mod edit;
mod scope;

use std::path::{Path, PathBuf};

use serde_json::json;
use sprawl_guard_lib::check::check_without_ratchet;
use sprawl_guard_lib::config::{LoadOptions, load_resolved_config};
use sprawl_guard_lib::{
    CheckOptions, CheckReport, CheckViolation, CodeLines, DirectoryRatchet, DirectoryViolationKind,
    FileRatchet, FileViolationKind, RatchetFile, RatchetVersion, TestDirectoryRatchet,
    TestFileRatchet,
};

use super::{BaselineArgs, RatchetArgs};
use crate::error::{CliError, Result};
use edit::{lower_ratchet_scope, ratchet_raise_count, replace_ratchet_scope, sort_ratchet};
use scope::{BaselineScopeRequest, ratchet_update_scope, replacement_scope};

pub(super) fn execute_baseline(
    root: Option<PathBuf>,
    config_path: Option<PathBuf>,
    args: BaselineArgs,
) -> Result<RatchetCommandExecution> {
    if args.all && !args.paths.is_empty() {
        return Err(CliError::AllWithPaths);
    }
    let context = RatchetCommandContext::load(root, config_path)?;
    let ratchet_exists = context.ratchet_path.try_exists().map_err(|source| {
        sprawl_guard_lib::SprawlError::ReadFileType {
            path: context.ratchet_path.clone(),
            source,
        }
    })?;
    if ratchet_exists && args.paths.is_empty() && !args.all {
        return Err(CliError::AmbiguousRatchetReplacement);
    }

    let report = current_violation_report(&context, args.paths.clone())?;
    let new_entries = ratchet_from_report(&report)?;
    let next = if ratchet_exists {
        let previous = RatchetFile::read(&context.ratchet_path)?;
        let scope_request = if args.all {
            BaselineScopeRequest::All
        } else {
            BaselineScopeRequest::Paths
        };
        let scope = replacement_scope(&context.root, &args.paths, scope_request)?;
        let raises = ratchet_raise_count(&previous, &new_entries);
        if raises > 0 && !args.allow_raises {
            return Err(CliError::RatchetRaises { count: raises });
        }
        replace_ratchet_scope(previous, new_entries, &scope)
    } else {
        new_entries
    };

    finish_ratchet_command(
        RatchetCommandKind::Baseline,
        &context.ratchet_path,
        &next,
        args.write,
    )
}

pub(super) fn execute_ratchet(
    root: Option<PathBuf>,
    config_path: Option<PathBuf>,
    args: RatchetArgs,
) -> Result<RatchetCommandExecution> {
    let context = RatchetCommandContext::load(root, config_path)?;
    if !context.ratchet_path.try_exists().map_err(|source| {
        sprawl_guard_lib::SprawlError::ReadFileType {
            path: context.ratchet_path.clone(),
            source,
        }
    })? {
        return Err(CliError::MissingRatchetFile {
            path: context.ratchet_path,
        });
    }

    let previous = RatchetFile::read(&context.ratchet_path)?;
    let (scope, measured_paths) = ratchet_update_scope(&context.root, &args.paths, &previous)?;
    let current = if args.paths.is_empty() || !measured_paths.is_empty() {
        ratchet_from_report(&current_violation_report(&context, measured_paths)?)?
    } else {
        RatchetFile::default()
    };
    let next = lower_ratchet_scope(previous, current, &scope);

    finish_ratchet_command(
        RatchetCommandKind::Ratchet,
        &context.ratchet_path,
        &next,
        args.write,
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RatchetCommandKind {
    Baseline,
    Ratchet,
}

pub(super) struct RatchetCommandExecution {
    kind: RatchetCommandKind,
    path: PathBuf,
    ratchet: RatchetFile,
    wrote: bool,
}

pub(super) fn render_ratchet_stdout(
    execution: &RatchetCommandExecution,
    quiet: bool,
) -> Result<String> {
    if quiet {
        return Ok(String::new());
    }
    if execution.wrote {
        return Ok(format!("wrote {}\n", execution.path.display()));
    }
    toml::to_string_pretty(&execution.ratchet).map_err(|source| CliError::RenderConfig { source })
}

pub(super) fn ratchet_payload(execution: &RatchetCommandExecution) -> serde_json::Value {
    json!({
        "kind": match execution.kind {
            RatchetCommandKind::Baseline => "baseline",
            RatchetCommandKind::Ratchet => "ratchet",
        },
        "path": execution.path,
        "ratchet": execution.ratchet,
        "wrote": execution.wrote,
    })
}

struct RatchetCommandContext {
    root: PathBuf,
    config: sprawl_guard_lib::config::Config,
    ratchet_path: PathBuf,
}

impl RatchetCommandContext {
    fn load(root: Option<PathBuf>, config_path: Option<PathBuf>) -> Result<Self> {
        let root = root.unwrap_or(
            std::env::current_dir().map_err(|source| CliError::CurrentDirectory { source })?,
        );
        let loaded = load_resolved_config(&LoadOptions {
            root: root.clone(),
            config: config_path,
        })?;
        let ratchet_path = root.join(loaded.config.ratchet.file.as_str());
        Ok(Self {
            root,
            config: loaded.config,
            ratchet_path,
        })
    }
}

fn current_violation_report(
    context: &RatchetCommandContext,
    paths: Vec<PathBuf>,
) -> Result<CheckReport> {
    Ok(check_without_ratchet(CheckOptions {
        root: context.root.clone(),
        config: &context.config,
        paths,
        require_ratchet_current: Some(false),
        traversal_error_policy: None,
    })?)
}

fn ratchet_from_report(report: &CheckReport) -> Result<RatchetFile> {
    let mut ratchet = RatchetFile {
        version: RatchetVersion::current(),
        files: Vec::new(),
        test_files: Vec::new(),
        directories: Vec::new(),
        test_directories: Vec::new(),
    };
    for violation in report.violations() {
        match violation {
            CheckViolation::FileLoc(violation) => match violation.kind {
                FileViolationKind::FileLoc => ratchet.files.push(FileRatchet {
                    path: violation.path.clone(),
                    non_test_code_lines: CodeLines::new(violation.actual.get())
                        .expect("violating file LOC must be positive"),
                }),
                FileViolationKind::TestFileLoc => ratchet.test_files.push(TestFileRatchet {
                    path: violation.path.clone(),
                    code_lines: CodeLines::new(violation.actual.get())
                        .expect("violating test file LOC must be positive"),
                }),
            },
            CheckViolation::DirectoryFanout(violation) => match violation.kind {
                DirectoryViolationKind::DirectoryFanout => {
                    ratchet.directories.push(DirectoryRatchet {
                        path: violation.path.clone(),
                        leaf_source_files: violation.actual,
                    });
                }
                DirectoryViolationKind::TestDirectoryFanout => {
                    ratchet.test_directories.push(TestDirectoryRatchet {
                        path: violation.path.clone(),
                        leaf_test_files: violation.actual,
                    });
                }
            },
            CheckViolation::Ratchet(_) => {}
        }
    }
    sort_ratchet(&mut ratchet);
    Ok(ratchet)
}

fn finish_ratchet_command(
    kind: RatchetCommandKind,
    path: &Path,
    ratchet: &RatchetFile,
    write: bool,
) -> Result<RatchetCommandExecution> {
    if write {
        ratchet.write(path)?;
        return Ok(RatchetCommandExecution {
            kind,
            path: path.to_path_buf(),
            ratchet: ratchet.clone(),
            wrote: true,
        });
    }
    Ok(RatchetCommandExecution {
        kind,
        path: path.to_path_buf(),
        ratchet: ratchet.clone(),
        wrote: false,
    })
}
