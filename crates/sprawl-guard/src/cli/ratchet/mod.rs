mod edit;
mod preview;
mod scope;

use std::path::{Path, PathBuf};

use sprawl_guard_lib::check::check_without_ratchet;
use sprawl_guard_lib::config::{LoadOptions, load_resolved_config};
use sprawl_guard_lib::{
    CheckOptions, CheckReport, CheckViolation, DirectoryRatchet, FileRatchet, LocLimit,
    RatchetFile, RatchetSchemaVersion, TraversalErrorPolicy,
};

use super::{RatchetFormat, TightenArgs, UpdateArgs};
use crate::error::{CliError, Result};
use edit::{lower_ratchet_scope, update_ratchet};
use scope::tighten_scope;

pub(super) fn execute_update(
    root: Option<PathBuf>,
    config_path: Option<PathBuf>,
    args: UpdateArgs,
) -> Result<String> {
    let ceiling_update_policy = args.ceiling_update_policy();
    let context = RatchetCommandContext::load(root, config_path)?;
    let ratchet_exists = context.ratchet_path.try_exists().map_err(|source| {
        sprawl_guard_lib::SprawlError::ReadFileType {
            path: context.ratchet_path.clone(),
            source,
        }
    })?;
    let report = current_violation_report(&context, args.paths.clone())?;
    let new_entries = ratchet_from_report(&report)?;
    let previous = if ratchet_exists {
        let mut previous = RatchetFile::read(&context.ratchet_path)?;
        previous.sort();
        Some(previous)
    } else {
        None
    };
    let next = update_ratchet(
        previous.clone().unwrap_or_default(),
        new_entries,
        ceiling_update_policy,
    );

    finish_ratchet_command(
        &context.ratchet_path,
        previous.as_ref(),
        &next,
        args.write,
        args.format,
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CeilingUpdatePolicy {
    Preserve,
    AllowIncreases,
}

impl CeilingUpdatePolicy {
    const fn allows_increases(self) -> bool {
        matches!(self, Self::AllowIncreases)
    }
}

pub(super) fn execute_tighten(
    root: Option<PathBuf>,
    config_path: Option<PathBuf>,
    args: TightenArgs,
) -> Result<String> {
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

    let mut previous = RatchetFile::read(&context.ratchet_path)?;
    previous.sort();
    let (scope, measured_paths) = tighten_scope(&context.root, &args.paths, &previous)?;
    let current = if args.paths.is_empty() || !measured_paths.is_empty() {
        let report = current_violation_report(&context, measured_paths)?;
        ratchet_from_report(&report)?
    } else {
        RatchetFile::default()
    };
    let next = lower_ratchet_scope(previous.clone(), current, &scope)?;

    finish_ratchet_command(
        &context.ratchet_path,
        Some(&previous),
        &next,
        args.write,
        args.format,
    )
}

struct RatchetCommandContext {
    root: PathBuf,
    loaded: sprawl_guard_lib::config::LoadedConfig,
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
            loaded,
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
        policy: context.loaded.resolved_policy(),
        paths,
        require_ratchet_current: Some(false),
        traversal_error_policy: Some(TraversalErrorPolicy::Fail),
    })?)
}

fn ratchet_from_report(report: &CheckReport) -> Result<RatchetFile> {
    let mut ratchet = RatchetFile {
        schema_version: RatchetSchemaVersion::CURRENT,
        files: Vec::new(),
        directories: Vec::new(),
    };
    for violation in report.violations() {
        match violation {
            CheckViolation::FileLoc(violation) => {
                ratchet.files.push(FileRatchet {
                    path: violation.path.clone(),
                    role: violation.kind.role(),
                    max_loc: LocLimit::new(violation.measurement.actual().get())
                        .expect("violating file LOC must be positive"),
                });
            }
            CheckViolation::DirectoryFanout(violation) => {
                ratchet.directories.push(DirectoryRatchet {
                    path: violation.path.clone(),
                    role: violation.kind.role(),
                    group: violation.group.clone(),
                    leaf_files: violation.actual,
                });
            }
            CheckViolation::Ratchet(_) => {}
        }
    }
    ratchet.sort();
    Ok(ratchet)
}

fn finish_ratchet_command(
    path: &Path,
    previous: Option<&RatchetFile>,
    ratchet: &RatchetFile,
    write: bool,
    format: RatchetFormat,
) -> Result<String> {
    let stdout = match format {
        RatchetFormat::Toml => ratchet.to_toml()?,
        RatchetFormat::Human => {
            let changes =
                preview::render_changes(previous.unwrap_or(&RatchetFile::default()), ratchet)?;
            if changes.is_empty() {
                match (previous, write) {
                    (None, true) => format!("Created empty ratchet at {path:?}.\n"),
                    (None, false) => format!(
                        "No ceiling changes; --write will create an empty ratchet at {path:?}.\n"
                    ),
                    (Some(_), _) => "No ratchet changes.\n".to_owned(),
                }
            } else {
                let action = if write {
                    format!("Wrote {path:?}.\n")
                } else {
                    "Preview only; use --write to apply.\n".to_owned()
                };
                format!("Ratchet changes:\n{changes}\n{action}")
            }
        }
    };
    if write && previous != Some(ratchet) {
        ratchet.write(path)?;
    }
    Ok(stdout)
}
