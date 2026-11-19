mod edit;
mod import;
mod preview;
mod refusal;
mod scope;

pub(super) use refusal::render_refusal;

use std::path::PathBuf;

use sprawl_guard_lib::check::check_without_ratchet;
use sprawl_guard_lib::config::load_resolved_config;
use sprawl_guard_lib::{
    CheckOptions, CheckReport, CheckViolation, DirectoryRatchet, FileRatchet, FileSizeRatchet,
    LineLimit, RatchetFile, RatchetSchemaVersion, SprawlError, UnreadablePathPolicy,
};

use super::{ProjectArgs, RatchetFormat, TightenArgs, UpdateArgs};
use crate::error::{CliError, Result};
use edit::{lower_ratchet_scope, update_ratchet};
use scope::tighten_scope;

pub(super) fn execute_update(default_root: PathBuf, args: UpdateArgs) -> Result<String> {
    let ceiling_update_policy = args.ceiling_update_policy();
    let context = RatchetCommandContext::load(args.project, default_root)?;
    let report = current_violation_report(&context, args.paths)?;
    let import = args
        .import
        .map(|linter| {
            let root = super::init::resolve_cli_root(context.root.clone())?;
            import::Suppressions::load(&root, report.violations(), linter)
        })
        .transpose()?;
    let ratchet_exists = context.ratchet_path.try_exists().map_err(|source| {
        sprawl_guard_lib::SprawlError::ReadFileType {
            path: context.ratchet_path.clone(),
            source,
        }
    })?;
    let new_entries = ratchet_from_report(&report, import.as_ref());
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
        let mut current = ratchet_from_report(&report, None);
        keep_uncounted_ceilings(&mut current, &previous, &report);
        current
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

/// Checks the selected paths without the ratchet file, for a command that writes it.
///
/// Fails when any path could not be read: a ceiling written from that run would drop the entries
/// of every file below it.
fn current_violation_report(
    context: &RatchetCommandContext,
    paths: Vec<PathBuf>,
) -> Result<CheckReport> {
    let report = check_without_ratchet(CheckOptions {
        root: context.root.clone(),
        policy: context.loaded.resolved_config(),
        paths,
        require_ratchet_current: Some(false),
        unreadable_paths: Some(UnreadablePathPolicy::Fail),
    })?;
    if !report.unreadable_paths().is_empty() {
        return Err(SprawlError::TraversalFailed {
            root: PathBuf::from(report.root()),
            unreadable_paths: report.unreadable_paths().to_vec(),
        }
        .into());
    }
    Ok(report)
}

/// Adds the previous ceilings of files whose lines `report` did not count, so that tightening
/// keeps them instead of removing them as no longer violating.
fn keep_uncounted_ceilings(
    current: &mut RatchetFile,
    previous: &RatchetFile,
    report: &CheckReport,
) {
    current.files.extend(
        previous
            .files
            .iter()
            .filter(|entry| report.lines_uncounted(&entry.path, entry.role))
            .cloned(),
    );
    current.sort();
}

fn ratchet_from_report(report: &CheckReport, import: Option<&import::Suppressions>) -> RatchetFile {
    let mut ratchet = RatchetFile {
        schema_version: RatchetSchemaVersion::CURRENT,
        files: Vec::new(),
        directories: Vec::new(),
        file_sizes: Vec::new(),
    };
    for violation in report.violations() {
        match violation {
            CheckViolation::FileLines(violation) => {
                if import.is_some_and(|import| !import.covers(&violation.path)) {
                    continue;
                }
                ratchet.files.push(FileRatchet {
                    path: violation.path.clone(),
                    role: violation.kind.role(),
                    max_lines: LineLimit::new(violation.measurement.actual().get())
                        .expect("a violating file has at least one line"),
                });
            }
            CheckViolation::DirectoryDirectFiles(violation) if import.is_none() => {
                ratchet.directories.push(DirectoryRatchet {
                    path: violation.path.clone(),
                    role: violation.kind.role(),
                    group: violation.group.clone(),
                    max_direct_files: violation.actual,
                });
            }
            CheckViolation::FileBytes(violation) if import.is_none() => {
                // A file over a positive limit is never empty, so it always has a ceiling.
                if let Some(max_bytes) = violation.actual.as_limit() {
                    ratchet.file_sizes.push(FileSizeRatchet {
                        path: violation.path.clone(),
                        storage: violation.kind.storage(),
                        max_bytes,
                    });
                }
            }
            CheckViolation::Ratchet(_)
            | CheckViolation::DirectoryDirectFiles(_)
            | CheckViolation::FileBytes(_) => {}
        }
    }
    ratchet.sort();
    ratchet
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
