mod edit;
mod preview;
mod scope;

use std::path::PathBuf;

use sprawl_guard_lib::check::check_without_ratchet;
use sprawl_guard_lib::config::load_resolved_config;
use sprawl_guard_lib::{
    CheckOptions, CheckReport, CheckViolation, DirectoryRatchet, FileRatchet, LineLimit,
    RatchetFile, RatchetSchemaVersion, UnreadablePathPolicy,
};

use super::{ProjectArgs, RatchetFormat, TightenArgs, UpdateArgs};
use crate::error::{CliError, Result};
use edit::{lower_ratchet_scope, update_ratchet};
use scope::tighten_scope;

pub(super) fn execute_update(default_root: PathBuf, args: UpdateArgs) -> Result<String> {
    let ceiling_update_policy = args.ceiling_update_policy();
    let context = RatchetCommandContext::load(args.project, default_root)?;
    let ratchet_exists = context.ratchet_path.try_exists().map_err(|source| {
        sprawl_guard_lib::SprawlError::ReadFileType {
            path: context.ratchet_path.clone(),
            source,
        }
    })?;
    let report = current_violation_report(&context, args.paths)?;
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
        &context,
        previous.as_ref(),
        &next,
        args.write,
        args.format.into(),
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

pub(super) fn execute_tighten(default_root: PathBuf, args: TightenArgs) -> Result<String> {
    let context = RatchetCommandContext::load(args.project, default_root)?;
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
        &context,
        Some(&previous),
        &next,
        args.write,
        args.format.into(),
    )
}

struct RatchetCommandContext {
    root: PathBuf,
    loaded: sprawl_guard_lib::config::LoadedConfig,
    ratchet_path: PathBuf,
}

impl RatchetCommandContext {
    fn load(project: ProjectArgs, default_root: PathBuf) -> Result<Self> {
        let options = project.load_options(default_root);
        let loaded = load_resolved_config(&options)?;
        let ratchet_path = options.root.join(loaded.config.ratchet.file.as_str());
        Ok(Self {
            root: options.root,
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
        policy: context.loaded.resolved_config(),
        paths,
        require_ratchet_current: Some(false),
        unreadable_paths: Some(UnreadablePathPolicy::Fail),
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
            CheckViolation::FileLines(violation) => {
                ratchet.files.push(FileRatchet {
                    path: violation.path.clone(),
                    role: violation.kind.role(),
                    max_lines: LineLimit::new(violation.measurement.actual().get())
                        .expect("a violating file has at least one line"),
                });
            }
            CheckViolation::DirectoryDirectFiles(violation) => {
                ratchet.directories.push(DirectoryRatchet {
                    path: violation.path.clone(),
                    role: violation.kind.role(),
                    group: violation.group.clone(),
                    max_direct_files: violation.actual,
                });
            }
            CheckViolation::Ratchet(_) => {}
        }
    }
    ratchet.sort();
    Ok(ratchet)
}

fn finish_ratchet_command(
    context: &RatchetCommandContext,
    previous: Option<&RatchetFile>,
    ratchet: &RatchetFile,
    write: bool,
    format: RatchetFormat,
) -> Result<String> {
    let path = &context.ratchet_path;
    let shown_path = super::display_path(&context.root, path);
    let stdout = match format {
        RatchetFormat::Toml => ratchet.to_toml()?,
        RatchetFormat::Human => {
            let changes =
                preview::render_changes(previous.unwrap_or(&RatchetFile::default()), ratchet)?;
            if changes.is_empty() {
                match (previous, write) {
                    (None, true) => format!("Created empty ratchet file {shown_path}\n"),
                    (None, false) => {
                        format!(
                            "No ratchet changes; --write will create an empty ratchet file {shown_path}\n"
                        )
                    }
                    (Some(_), _) => "No ratchet changes.\n".to_owned(),
                }
            } else {
                let action = if write {
                    format!("Wrote {shown_path}\n")
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
