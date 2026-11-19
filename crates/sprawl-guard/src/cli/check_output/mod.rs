mod github;
mod listing;
mod sarif;

use std::path::Path;

use sprawl_guard_lib::check::{
    CheckViolation, CheckWarning, CheckWarningKind, DirectoryDirectFilesViolation,
    FileLinesViolation, RatchetViolation, RatchetViolationKind,
};
use sprawl_guard_lib::{CheckReport, CheckStatus};
use sprawl_guard_lib::{
    ConfigSource, FiniteOrUnlimited, MaxFindings, RelativePath, UncountedFile, UnreadablePath,
    UnreadablePathKind, UnreadablePathSeverity,
};

use super::{CheckFormat, display_relative};
use crate::error::{CliError, Result};
use listing::{FindingListing, ViolationGroup, violations_in_display_order};

#[cfg(test)]
fn missing_directory_ratchet(
    group: sprawl_guard_lib::LanguageGroupId,
    ratchet: u32,
) -> CheckViolation {
    use sprawl_guard_lib::check::DirectoryViolationKind;
    use sprawl_guard_lib::{
        DirectoryRatchetStaleMissingViolation, RatchetCount, RatchetStaleMissingViolation,
    };

    CheckViolation::Ratchet(RatchetViolation::Missing(
        RatchetStaleMissingViolation::Directory(DirectoryRatchetStaleMissingViolation {
            kind: DirectoryViolationKind::DirectoryDirectFiles,
            path: sprawl_guard_lib::RelativePath::new("src").unwrap(),
            group,
            ratchet: RatchetCount::new(ratchet),
        }),
    ))
}

/// Tells the reader how to see what the finding cap left out.
const SEE_ALL_HINT: &str = "Use --max-findings unlimited to see all.";

pub(super) fn render_check_report(
    report: &CheckReport,
    format: CheckFormat,
    quiet: bool,
    max_findings: FiniteOrUnlimited<MaxFindings>,
) -> Result<String> {
    let listing = FindingListing::new(report, quiet, max_findings);
    match format {
        CheckFormat::Human => Ok(human_check_report(report, quiet, listing)),
        CheckFormat::Github => Ok(github::github_check_report(
            &human_check_report(report, quiet, listing),
            report,
            listing,
        )),
        CheckFormat::Json => serde_json::to_string_pretty(report)
            .map(|output| format!("{output}\n"))
            .map_err(|source| CliError::RenderJson { source }),
        CheckFormat::Sarif => serde_json::to_string_pretty(&sarif::sarif_check_report(report))
            .map(|output| format!("{output}\n"))
            .map_err(|source| CliError::RenderJson { source }),
    }
}

fn human_check_report(report: &CheckReport, quiet: bool, listing: FindingListing) -> String {
    let mut output = String::new();
    let status = report.status();
    if quiet && status == CheckStatus::Passed {
        return output;
    }
    write_config_source_notice(&mut output, report);
    if !quiet {
        write_warnings(&mut output, report, listing.warnings);
    }
    if report.has_violations() {
        write_violation_groups(&mut output, report, listing);
    } else {
        output.push_str(match (status, report.selected_paths().is_all()) {
            (CheckStatus::Error, _) => {
                "No sprawl-guard violations found in the files that could be analyzed.\n"
            }
            (_, true) => "No sprawl-guard violations found.\n",
            (_, false) => "No sprawl-guard violations found in the selected paths.\n",
        });
        write_scan_summary(&mut output, report);
    }
    if !quiet || status == CheckStatus::Error {
        write_unreadable_paths(&mut output, report, listing.unreadable_paths);
        write_uncounted_files(&mut output, report, listing.uncounted_files);
    }
    if status == CheckStatus::Error {
        start_section(&mut output);
        output.push_str(INCOMPLETE_ANALYSIS_NOTE);
    }
    write_truncation_footer(&mut output, listing.listed(), listing.total);
    output
}

/// Explains why a run without violations still failed, and how to change that.
const INCOMPLETE_ANALYSIS_NOTE: &str = "The check failed because it could not analyze everything \
    it selected. Fix the paths listed above, or set traversal.unreadable_paths = \"warn\" to report them \
    without failing.\n";

/// Says how many findings the report lists, when the cap left some out.
fn write_truncation_footer(output: &mut String, listed: usize, total: usize) {
    if listed < total {
        start_section(output);
        output.push_str(&format!(
            "Showing {listed} of {total} findings. {SEE_ALL_HINT}\n"
        ));
    }
}

/// Separates a new section from earlier output with one blank line.
fn start_section(output: &mut String) {
    if !output.is_empty() && !output.ends_with("\n\n") {
        output.push('\n');
    }
}

/// Starts a section whose heading shows how many findings it has, listed or not.
fn write_section_heading(output: &mut String, heading: &str, total: usize) {
    start_section(output);
    output.push_str(&format!("{heading} ({total}):\n\n"));
}

/// Names how many of a section's findings the cap left out, if any.
fn unlisted_message(total: usize, listed: usize) -> Option<String> {
    let unlisted = total - listed;
    (unlisted > 0).then(|| format!("  … and {unlisted} more\n"))
}

fn write_config_source_notice(output: &mut String, report: &CheckReport) {
    output.push_str(&config_source_notice(
        report.config_source(),
        Path::new(report.root()),
    ));
}

fn config_source_notice(config_source: &ConfigSource, root: &Path) -> String {
    match config_source {
        ConfigSource::BuiltIn => format!(
            "Configuration: built-in defaults (no sprawl-guard.toml in {})\n\n",
            root.display()
        ),
        ConfigSource::Authored { path } => format!(
            "Configuration: {}\n\n",
            super::display_path(root, path.as_path())
        ),
    }
}

fn write_scan_summary(output: &mut String, report: &CheckReport) {
    let summary = report.summary();
    let files_checked = summary.files_checked.into_inner();
    let directories_checked = summary.directories_checked.into_inner();
    let languages_detected = summary.languages_detected.into_inner();
    output.push_str(&format!(
        "Checked {} source {} in {} {} across {} {}.\n",
        files_checked,
        plural(files_checked, "file", "files"),
        directories_checked,
        plural(directories_checked, "directory", "directories"),
        languages_detected,
        plural(languages_detected, "language", "languages"),
    ));
    let ratchet_entries = summary.ratchet_entries.into_inner();
    if ratchet_entries > 0 && report.status() == CheckStatus::Passed {
        output.push_str(&format!(
            "Passed with {ratchet_entries} ratchet {} remaining.\n",
            plural(ratchet_entries, "entry", "entries"),
        ));
    }
}

/// Picks the singular or plural form of a word for `count`.
pub(super) fn plural<'word>(count: u32, singular: &'word str, plural: &'word str) -> &'word str {
    if count == 1 { singular } else { plural }
}

fn write_warnings(output: &mut String, report: &CheckReport, listed: usize) {
    let warnings = report.warnings();
    if warnings.is_empty() {
        return;
    }
    write_section_heading(output, "Warnings", warnings.len());
    for warning in &warnings[..listed] {
        output.push_str("  ");
        output.push_str(&warning_message(warning));
        output.push('\n');
    }
    output.extend(unlisted_message(warnings.len(), listed));
    output.push('\n');
}

fn warning_message(warning: &CheckWarning) -> String {
    match warning.kind {
        CheckWarningKind::RustCfgTestParseFailed => format!(
            "{}: could not parse Rust to leave out cfg(test) code, so all of it was counted: {}",
            display_relative(warning.path.as_str()),
            warning.detail
        ),
    }
}

fn write_unreadable_paths(output: &mut String, report: &CheckReport, listed: usize) {
    let unreadable_paths = report.unreadable_paths();
    if unreadable_paths.is_empty() {
        return;
    }
    write_section_heading(output, "Skipped unreadable paths", unreadable_paths.len());
    for unreadable_path in &unreadable_paths[..listed] {
        output.push_str("  ");
        output.push_str(&unreadable_path_message(unreadable_path));
        output.push('\n');
    }
    output.extend(unlisted_message(unreadable_paths.len(), listed));
}

fn write_uncounted_files(output: &mut String, report: &CheckReport, listed: usize) {
    let uncounted_files = report.uncounted_files();
    if uncounted_files.is_empty() {
        return;
    }
    write_section_heading(
        output,
        "Files too large to count lines",
        uncounted_files.len(),
    );
    for file in &uncounted_files[..listed] {
        output.push_str("  ");
        output.push_str(&uncounted_file_message(file));
        output.push('\n');
    }
    output.extend(unlisted_message(uncounted_files.len(), listed));
}

/// Describes one source file whose lines were not counted, on a single line.
fn uncounted_file_message(file: &UncountedFile) -> String {
    format!(
        "{}: {} bytes is over traversal.max_parsed_code_file_bytes ({}), so its lines were not counted",
        display_relative(file.path.as_str()),
        file.size,
        file.limit,
    )
}

/// One gap in a check's analysis, as machine-readable output reports it.
pub(super) struct GapNotice<'report> {
    /// Whether the gap fails the check.
    pub(super) severity: UnreadablePathSeverity,
    /// The file the gap is about, when it is a file the user can open.
    pub(super) file: Option<&'report RelativePath>,
    /// One-line description that names the path.
    pub(super) message: String,
}

/// Returns the first `unreadable_paths` unreadable paths and the first `uncounted_files`
/// uncounted files of the report, in the order the human report lists them.
pub(super) fn gap_notices(
    report: &CheckReport,
    unreadable_paths: usize,
    uncounted_files: usize,
) -> impl Iterator<Item = GapNotice<'_>> {
    let unreadable = report.unreadable_paths()[..unreadable_paths]
        .iter()
        .map(|path| GapNotice {
            severity: path.severity,
            file: None,
            message: unreadable_path_message(path),
        });
    let uncounted = report.uncounted_files()[..uncounted_files]
        .iter()
        .map(|file| GapNotice {
            severity: file.severity,
            file: Some(&file.path),
            message: uncounted_file_message(file),
        });
    unreadable.chain(uncounted)
}

/// Describes one unreadable path on a single line: what failed, where, and the backend detail.
pub(super) fn unreadable_path_message(unreadable_path: &UnreadablePath) -> String {
    let path = display_relative(
        unreadable_path
            .path
            .as_ref()
            .map_or("", |path| path.as_str()),
    );
    format!(
        "{} at {}: {}",
        unreadable_path_kind_message(unreadable_path.kind),
        path,
        unreadable_path.detail
    )
}

fn unreadable_path_kind_message(kind: UnreadablePathKind) -> &'static str {
    match kind {
        UnreadablePathKind::ReadDirFailed => "failed to read directory",
        UnreadablePathKind::FileTypeFailed => "failed to read file type",
        UnreadablePathKind::SymlinkLoop => "symlink loop found",
        UnreadablePathKind::BrokenSymlink => "broken symlink",
        UnreadablePathKind::IgnoreParseFailed => "failed to parse ignore rule",
        UnreadablePathKind::ReadFileFailed => "failed to read source file",
    }
}

/// Writes each violation group, listing the violations that `listing` allows in display order.
fn write_violation_groups(output: &mut String, report: &CheckReport, listing: FindingListing) {
    let violations = violations_in_display_order(report);
    let mut remaining = listing.violations;
    for violations in violations.chunk_by(|a, b| ViolationGroup::of(a) == ViolationGroup::of(b)) {
        let group = ViolationGroup::of(violations[0]);
        let listed = violations.len().min(remaining);
        remaining -= listed;
        write_section_heading(output, group.heading(), violations.len());
        for violation in &violations[..listed] {
            output.push_str(&violation_entry(violation, listing.directory_files));
        }
        if let Some(message) = unlisted_message(violations.len(), listed) {
            output.push_str(&message);
            output.push('\n');
        }
        if group == ViolationGroup::StaleRatchet {
            output.push_str("Run `sprawl-guard tighten --write` to lower or remove them.\n");
        }
    }
}

fn violation_entry(violation: &CheckViolation, directory_files: usize) -> String {
    match violation {
        CheckViolation::FileLines(violation) => violation_message(violation),
        CheckViolation::DirectoryDirectFiles(violation) => {
            directory_violation_message(violation, directory_files)
        }
        CheckViolation::Ratchet(violation) => ratchet_violation_message(violation),
    }
}

fn violation_message(violation: &FileLinesViolation) -> String {
    format!(
        "  {}: {} lines (limit {}{})\n    language: {}, counting {}\n\n",
        display_relative(violation.path.as_str()),
        violation.measurement.actual().get(),
        violation.limit,
        ceiling_message(violation.ratchet),
        violation.language,
        violation.measurement.count_lines(),
    )
}

/// Formats the ratchet ceiling that a violation grew above, if it has one.
pub(super) fn ceiling_message(ceiling: Option<impl std::fmt::Display>) -> String {
    ceiling
        .map(|ceiling| format!(", ceiling {ceiling}"))
        .unwrap_or_default()
}

fn ratchet_violation_message(violation: &RatchetViolation) -> String {
    let (description, unit) = ratchet_kind_message(violation.kind());
    let actual = violation
        .actual()
        .map(|value| format!("now {} {unit}", value.get()))
        .unwrap_or_else(|| "no current violation".to_owned());
    let limit = violation
        .limit()
        .map(|value| format!(", limit {value}"))
        .unwrap_or_default();
    let group = violation
        .group()
        .map(|group| format!("    group: {group}\n"))
        .unwrap_or_default();
    let count_lines = violation
        .line_measurement()
        .map(|measurement| format!("    counting {}\n", measurement.count_lines()))
        .unwrap_or_default();
    format!(
        "  {}: {description}\n    {actual} (ceiling {}{limit})\n{group}{count_lines}\n",
        display_relative(violation.path().as_str()),
        violation.ratchet().get()
    )
}

/// Names the role of a stale ratchet entry and the unit of its measurement.
fn ratchet_kind_message(kind: RatchetViolationKind) -> (&'static str, &'static str) {
    match kind {
        RatchetViolationKind::FileLinesStale => ("file", "lines"),
        RatchetViolationKind::TestFileLinesStale => ("test file", "lines"),
        RatchetViolationKind::DirectoryDirectFilesStale => ("directory", "direct files"),
        RatchetViolationKind::TestDirectoryDirectFilesStale => ("test directory", "direct files"),
    }
}

/// Describes a directory with too many direct files, naming up to `listed_files` of them.
fn directory_violation_message(
    violation: &DirectoryDirectFilesViolation,
    listed_files: usize,
) -> String {
    let files = violation
        .files
        .iter()
        .take(listed_files)
        .map(|file| format!("    {}\n", display_relative(file.as_str())))
        .collect::<String>();
    let unlisted = violation.files.len().saturating_sub(listed_files);
    let unlisted = if unlisted > 0 {
        format!("    … and {unlisted} more. {SEE_ALL_HINT}\n")
    } else {
        String::new()
    };
    format!(
        "  {}: {} direct files (limit {}{}, group: {})\n{files}{unlisted}\n",
        display_relative(violation.path.as_str()),
        violation.actual,
        violation.limit,
        ceiling_message(violation.ratchet),
        violation.group,
    )
}

#[cfg(test)]
mod tests {
    use indoc::indoc;
    use serde_json::json;
    use sprawl_guard_lib::check::FileViolationKind;
    use sprawl_guard_lib::{
        AnalysisCoverage, CheckViolation, CheckWarning, CheckWarningKind, CheckedDirectoryCount,
        CheckedFileCount, DetectedLanguageCount, DiagnosticDetail, DirectFiles,
        DirectoryDirectFilesViolation, DirectoryRatchetStaleMeasuredViolation,
        DirectoryViolationKind, FileLinesViolation, FileRatchetStaleMeasuredViolation,
        LanguageGroupId, LineCount, LineLimit, RatchetCount, RatchetEntryCount, RatchetLimit,
        RatchetStaleMeasuredViolation, RatchetViolation, RelativePath, ScanSummary,
        UnreadablePathSeverity,
    };

    use super::*;

    /// Renders the human report under `max_findings`, as `render_check_report` does.
    fn render_human(
        report: &CheckReport,
        quiet: bool,
        max_findings: FiniteOrUnlimited<MaxFindings>,
    ) -> String {
        human_check_report(
            report,
            quiet,
            FindingListing::new(report, quiet, max_findings),
        )
    }

    fn report_root() -> sprawl_guard_lib::ReportRoot {
        sprawl_guard_lib::ReportRoot::trusted("/repo")
    }

    fn counter_info() -> sprawl_guard_lib::check::CounterInfo {
        sprawl_guard_lib::check::CounterInfo {
            backend: "tokei",
            version: "14.0.0",
        }
    }

    fn scan_summary() -> ScanSummary {
        ScanSummary::new(
            CheckedFileCount::new(3),
            CheckedDirectoryCount::new(2),
            DetectedLanguageCount::new(1),
            RatchetEntryCount::new(0),
        )
    }

    fn report_with(violations: Vec<FileLinesViolation>) -> CheckReport {
        CheckReport::complete(
            report_root(),
            ConfigSource::BuiltIn,
            counter_info(),
            scan_summary(),
            vec![],
            violations
                .into_iter()
                .map(CheckViolation::FileLines)
                .collect(),
        )
    }

    fn violation(kind: FileViolationKind, path: &str) -> FileLinesViolation {
        FileLinesViolation {
            kind,
            path: RelativePath::new(path).unwrap(),
            language: sprawl_guard_lib::LanguageId::new("Rust").unwrap(),
            measurement: sprawl_guard_lib::code_line_counter::LineMeasurement::new(
                sprawl_guard_lib::CountLines::Code,
                sprawl_guard_lib::LineCounts::new(
                    LineCount::new(3),
                    LineCount::new(0),
                    LineCount::new(0),
                ),
            )
            .unwrap(),
            limit: LineLimit::new(2).unwrap(),
            ratchet: None,
        }
    }

    fn directory_violation(
        kind: DirectoryViolationKind,
        path: &str,
    ) -> DirectoryDirectFilesViolation {
        DirectoryDirectFilesViolation {
            kind,
            group: sprawl_guard_lib::LanguageGroupId::Source,
            path: RelativePath::new(path).unwrap(),
            actual: DirectFiles::new(3).unwrap(),
            limit: DirectFiles::new(2).unwrap(),
            ratchet: None,
            files: vec![
                RelativePath::new(format!("{path}/a.rs")).unwrap(),
                RelativePath::new(format!("{path}/b.rs")).unwrap(),
                RelativePath::new(format!("{path}/c.rs")).unwrap(),
            ],
        }
    }

    fn rust_cfg_warning() -> CheckWarning {
        CheckWarning {
            kind: CheckWarningKind::RustCfgTestParseFailed,
            path: RelativePath::new("src/lib.rs").unwrap(),
            detail: DiagnosticDetail::new("expected identifier"),
        }
    }

    fn report_with_rust_cfg_warning() -> CheckReport {
        CheckReport::complete(
            report_root(),
            ConfigSource::BuiltIn,
            counter_info(),
            scan_summary(),
            vec![rust_cfg_warning()],
            vec![],
        )
    }

    mod when_rendering_human_check_output {
        use super::*;

        #[test]
        fn it_groups_file_and_test_file_violations() {
            let report = report_with(vec![
                violation(FileViolationKind::FileLines, "src/lib.rs"),
                violation(FileViolationKind::TestFileLines, "tests/integration.rs"),
            ]);

            let output = render_human(&report, false, FiniteOrUnlimited::Unlimited);

            assert!(output.ends_with(indoc! {"
                Files over the line limit (1):

                  src/lib.rs: 3 lines (limit 2)
                    language: Rust, counting code

                Test files over the line limit (1):

                  tests/integration.rs: 3 lines (limit 2)
                    language: Rust, counting code

            "}));
        }

        #[test]
        fn it_groups_production_and_test_directory_violations() {
            let report = CheckReport::complete(
                report_root(),
                ConfigSource::BuiltIn,
                counter_info(),
                scan_summary(),
                vec![],
                vec![
                    CheckViolation::DirectoryDirectFiles(directory_violation(
                        DirectoryViolationKind::DirectoryDirectFiles,
                        "src",
                    )),
                    CheckViolation::DirectoryDirectFiles(directory_violation(
                        DirectoryViolationKind::TestDirectoryDirectFiles,
                        "tests",
                    )),
                ],
            );

            let output = render_human(&report, false, FiniteOrUnlimited::Unlimited);

            assert!(output.ends_with(indoc! {"
                Directories with too many direct files (1):

                  src: 3 direct files (limit 2, group: source)
                    src/a.rs
                    src/b.rs
                    src/c.rs

                Test directories with too many direct files (1):

                  tests: 3 direct files (limit 2, group: source)
                    tests/a.rs
                    tests/b.rs
                    tests/c.rs

            "}));
        }

        #[test]
        fn it_distinguishes_missing_directory_ratchets_for_groups_at_the_same_path() {
            let report = CheckReport::complete(
                report_root(),
                ConfigSource::BuiltIn,
                counter_info(),
                scan_summary(),
                vec![],
                vec![
                    missing_directory_ratchet(LanguageGroupId::Source, 4),
                    missing_directory_ratchet(LanguageGroupId::Documentation, 5),
                ],
            );

            let output = render_human(&report, false, FiniteOrUnlimited::Unlimited);

            assert_eq!(
                output,
                indoc! {"
                    Configuration: built-in defaults (no sprawl-guard.toml in /repo)

                    Stale ratchet entries (2):

                      src: directory
                        no current violation (ceiling 4)
                        group: source

                      src: directory
                        no current violation (ceiling 5)
                        group: documentation

                    Run `sprawl-guard tighten --write` to lower or remove them.
                "}
            );
        }

        #[test]
        fn it_identifies_the_group_and_unlimited_limit_for_a_measured_directory_ratchet() {
            let report = CheckReport::complete(
                report_root(),
                ConfigSource::BuiltIn,
                counter_info(),
                scan_summary(),
                vec![],
                vec![CheckViolation::Ratchet(RatchetViolation::Measured(
                    RatchetStaleMeasuredViolation::Directory(
                        DirectoryRatchetStaleMeasuredViolation {
                            kind: DirectoryViolationKind::DirectoryDirectFiles,
                            path: RelativePath::new("docs").unwrap(),
                            group: LanguageGroupId::Documentation,
                            actual: RatchetCount::new(2),
                            limit: RatchetLimit::Unlimited,
                            ratchet: RatchetCount::new(3),
                        },
                    ),
                ))],
            );

            let output = render_human(&report, false, FiniteOrUnlimited::Unlimited);

            assert!(output.contains("now 2 direct files (ceiling 3, limit unlimited)\n"));
            assert!(output.contains("group: documentation\n"));
        }

        #[test]
        fn it_reports_count_lines_for_a_measured_file_ratchet() {
            let report = CheckReport::complete(
                report_root(),
                ConfigSource::BuiltIn,
                counter_info(),
                scan_summary(),
                vec![],
                vec![CheckViolation::Ratchet(RatchetViolation::Measured(
                    RatchetStaleMeasuredViolation::File(FileRatchetStaleMeasuredViolation {
                        kind: FileViolationKind::FileLines,
                        path: RelativePath::new("src/lib.rs").unwrap(),
                        measurement: sprawl_guard_lib::code_line_counter::LineMeasurement::new(
                            sprawl_guard_lib::CountLines::CodeAndComments,
                            sprawl_guard_lib::LineCounts::new(
                                LineCount::new(3),
                                LineCount::new(2),
                                LineCount::new(1),
                            ),
                        )
                        .unwrap(),
                        limit: RatchetCount::new(2),
                        ratchet: RatchetCount::new(6),
                    }),
                ))],
            );

            let output = render_human(&report, false, FiniteOrUnlimited::Unlimited);

            assert!(output.ends_with(indoc! {"
                Stale ratchet entries (1):

                  src/lib.rs: file
                    now 5 lines (ceiling 6, limit 2)
                    counting code, comments

                Run `sprawl-guard tighten --write` to lower or remove them.
            "}));
        }

        #[test]
        fn it_reports_the_ceiling_of_a_file_that_grew_above_it() {
            let report = report_with(vec![FileLinesViolation {
                limit: LineLimit::new(1).unwrap(),
                ratchet: Some(LineLimit::new(2).unwrap()),
                ..violation(FileViolationKind::FileLines, "src/lib.rs")
            }]);

            let output = render_human(&report, false, FiniteOrUnlimited::Unlimited);

            assert!(output.contains("  src/lib.rs: 3 lines (limit 1, ceiling 2)\n"));
        }

        #[test]
        fn it_reports_the_ceiling_of_a_directory_that_grew_above_it() {
            let report = CheckReport::complete(
                report_root(),
                ConfigSource::BuiltIn,
                counter_info(),
                scan_summary(),
                vec![],
                vec![CheckViolation::DirectoryDirectFiles(
                    DirectoryDirectFilesViolation {
                        limit: DirectFiles::new(1).unwrap(),
                        ratchet: Some(DirectFiles::new(2).unwrap()),
                        ..directory_violation(DirectoryViolationKind::DirectoryDirectFiles, "src")
                    },
                )],
            );

            let output = render_human(&report, false, FiniteOrUnlimited::Unlimited);

            assert!(output.contains("  src: 3 direct files (limit 1, ceiling 2, group: source)\n"));
        }

        #[test]
        fn it_reports_success_when_there_are_no_violations() {
            let output = render_human(&report_with(vec![]), false, FiniteOrUnlimited::Unlimited);

            assert_eq!(
                output,
                concat!(
                    "Configuration: built-in defaults (no sprawl-guard.toml in /repo)\n\n",
                    "No sprawl-guard violations found.\n",
                    "Checked 3 source files in 2 directories across 1 language.\n",
                )
            );
        }

        #[test]
        fn it_reports_remaining_ratchet_entries_when_there_are_no_violations() {
            let report = CheckReport::complete(
                report_root(),
                ConfigSource::BuiltIn,
                counter_info(),
                ScanSummary::new(
                    CheckedFileCount::new(3),
                    CheckedDirectoryCount::new(2),
                    DetectedLanguageCount::new(1),
                    RatchetEntryCount::new(3),
                ),
                vec![],
                vec![],
            );

            let output = render_human(&report, false, FiniteOrUnlimited::Unlimited);

            assert!(output.ends_with(concat!(
                "Checked 3 source files in 2 directories across 1 language.\n",
                "Passed with 3 ratchet entries remaining.\n",
            )));
        }

        #[test]
        fn it_includes_warnings_before_the_status() {
            let report = report_with_rust_cfg_warning();

            let output = render_human(&report, false, FiniteOrUnlimited::Unlimited);

            assert_eq!(
                output,
                indoc! {"
                    Configuration: built-in defaults (no sprawl-guard.toml in /repo)

                    Warnings (1):

                      src/lib.rs: could not parse Rust to leave out cfg(test) code, so all of it was counted: expected identifier

                    No sprawl-guard violations found.
                    Checked 3 source files in 2 directories across 1 language.
                "}
            );
        }

        #[test]
        fn it_prints_nothing_for_a_passing_check_when_quiet() {
            let report = report_with_rust_cfg_warning();

            let output = render_human(&report, true, FiniteOrUnlimited::Unlimited);

            assert_eq!(output, "");
        }

        #[test]
        fn it_reports_unreadable_paths_after_the_success_status() {
            let report = CheckReport::with_coverage(
                report_root(),
                ConfigSource::BuiltIn,
                counter_info(),
                scan_summary(),
                vec![],
                AnalysisCoverage::new(
                    vec![UnreadablePath {
                        kind: UnreadablePathKind::ReadDirFailed,
                        severity: UnreadablePathSeverity::Warning,
                        path: Some(RelativePath::new("generated").unwrap()),
                        detail: DiagnosticDetail::new("permission denied"),
                    }],
                    vec![],
                ),
                vec![],
            );

            let output = render_human(&report, false, FiniteOrUnlimited::Unlimited);

            assert_eq!(
                output,
                indoc! {"
                    Configuration: built-in defaults (no sprawl-guard.toml in /repo)

                    No sprawl-guard violations found.
                    Checked 3 source files in 2 directories across 1 language.

                    Skipped unreadable paths (1):

                      failed to read directory at generated: permission denied
                "}
            );
        }
    }

    mod when_findings_exceed_the_cap {
        use super::*;

        fn cap(value: u32) -> FiniteOrUnlimited<MaxFindings> {
            FiniteOrUnlimited::Finite(MaxFindings::new(value).unwrap())
        }

        fn unreadable_path(path: &str) -> UnreadablePath {
            UnreadablePath {
                kind: UnreadablePathKind::ReadDirFailed,
                severity: UnreadablePathSeverity::Warning,
                path: Some(RelativePath::new(path).unwrap()),
                detail: DiagnosticDetail::new("permission denied"),
            }
        }

        fn report_with_every_section() -> CheckReport {
            CheckReport::with_coverage(
                report_root(),
                ConfigSource::BuiltIn,
                counter_info(),
                scan_summary(),
                vec![rust_cfg_warning()],
                AnalysisCoverage::new(
                    vec![unreadable_path("generated"), unreadable_path("vendor")],
                    vec![],
                ),
                vec![CheckViolation::FileLines(violation(
                    FileViolationKind::FileLines,
                    "src/lib.rs",
                ))],
            )
        }

        #[test]
        fn it_prints_warnings_then_violations_then_unreadable_paths() {
            let output = render_human(&report_with_every_section(), false, cap(3));

            assert!(output.ends_with(indoc! {"
                Warnings (1):

                  src/lib.rs: could not parse Rust to leave out cfg(test) code, so all of it was counted: expected identifier

                Files over the line limit (1):

                  src/lib.rs: 3 lines (limit 2)
                    language: Rust, counting code

                Skipped unreadable paths (2):

                  failed to read directory at generated: permission denied
                  … and 1 more

                Showing 3 of 4 findings. Use --max-findings unlimited to see all.
            "}));
        }

        #[test]
        fn it_lists_violations_before_warnings() {
            let output = render_human(&report_with_every_section(), false, cap(1));

            assert!(output.contains(indoc! {"
                Warnings (1):

                  … and 1 more

                Files over the line limit (1):

                  src/lib.rs: 3 lines (limit 2)
            "}));
        }

        #[test]
        fn it_annotates_a_violation_that_warnings_would_have_crowded_out() {
            let output = render_check_report(
                &report_with_every_section(),
                CheckFormat::Github,
                false,
                cap(1),
            )
            .unwrap();

            assert_eq!(output.matches("\n::error ").count(), 1);
            assert!(output.contains("::error file=src/lib.rs,"));
        }

        #[test]
        fn it_names_up_to_the_cap_of_a_directory_s_files() {
            let report = CheckReport::complete(
                report_root(),
                ConfigSource::BuiltIn,
                counter_info(),
                scan_summary(),
                vec![],
                vec![CheckViolation::DirectoryDirectFiles(directory_violation(
                    DirectoryViolationKind::DirectoryDirectFiles,
                    "src",
                ))],
            );

            let output = render_human(&report, false, cap(2));

            assert!(output.contains(concat!(
                "  src: 3 direct files (limit 2, group: source)\n",
                "    src/a.rs\n",
                "    src/b.rs\n",
                "    … and 1 more. Use --max-findings unlimited to see all.\n\n",
            )));
        }

        #[test]
        fn it_ends_a_passing_report_with_the_footer() {
            let report = CheckReport::with_coverage(
                report_root(),
                ConfigSource::BuiltIn,
                counter_info(),
                scan_summary(),
                vec![],
                AnalysisCoverage::new(
                    vec![unreadable_path("generated"), unreadable_path("vendor")],
                    vec![],
                ),
                vec![],
            );

            let output = render_human(&report, false, cap(1));

            assert!(output.ends_with(indoc! {"
                Checked 3 source files in 2 directories across 1 language.

                Skipped unreadable paths (2):

                  failed to read directory at generated: permission denied
                  … and 1 more

                Showing 1 of 2 findings. Use --max-findings unlimited to see all.
            "}));
        }

        #[test]
        fn it_leaves_out_findings_that_quiet_hides() {
            let output = render_human(&report_with_every_section(), true, cap(1));

            assert!(!output.contains("Showing"));
        }

        #[test]
        fn it_counts_the_violations_listed_in_display_order() {
            let report = report_with(vec![
                violation(FileViolationKind::TestFileLines, "tests/it.rs"),
                violation(FileViolationKind::FileLines, "src/lib.rs"),
            ]);

            let listed = violations_in_display_order(&report)
                .into_iter()
                .take(FindingListing::new(&report, false, cap(1)).violations)
                .collect::<Vec<_>>();

            assert_eq!(listed, vec![&report.violations()[1]]);
        }
    }

    mod when_analysis_has_gaps {
        use sprawl_guard_lib::FileBytes;
        use sprawl_guard_lib::config::ParsedCodeFileByteLimit;

        use super::*;

        fn uncounted_file(severity: UnreadablePathSeverity) -> UncountedFile {
            UncountedFile {
                kind: FileViolationKind::FileLines,
                severity,
                path: RelativePath::new("src/big.rs").unwrap(),
                size: FileBytes::new(20),
                limit: ParsedCodeFileByteLimit::try_new(8).unwrap(),
            }
        }

        fn report_with_gap(
            severity: UnreadablePathSeverity,
            violations: Vec<FileLinesViolation>,
        ) -> CheckReport {
            CheckReport::with_coverage(
                report_root(),
                ConfigSource::BuiltIn,
                counter_info(),
                ScanSummary::new(
                    CheckedFileCount::new(3),
                    CheckedDirectoryCount::new(2),
                    DetectedLanguageCount::new(1),
                    RatchetEntryCount::new(2),
                ),
                vec![rust_cfg_warning()],
                AnalysisCoverage::new(vec![], vec![uncounted_file(severity)]),
                violations
                    .into_iter()
                    .map(CheckViolation::FileLines)
                    .collect(),
            )
        }

        mod that_fail_the_check {
            use super::*;

            #[test]
            fn it_lists_the_file_and_ends_with_a_note_instead_of_the_ratchet_summary() {
                let report = report_with_gap(UnreadablePathSeverity::Error, vec![]);

                let output = render_human(&report, false, FiniteOrUnlimited::Unlimited);

                assert!(output.ends_with(indoc! {r#"
                    No sprawl-guard violations found in the files that could be analyzed.
                    Checked 3 source files in 2 directories across 1 language.

                    Files too large to count lines (1):

                      src/big.rs: 20 bytes is over traversal.max_parsed_code_file_bytes (8), so its lines were not counted

                    The check failed because it could not analyze everything it selected. Fix the paths listed above, or set traversal.unreadable_paths = "warn" to report them without failing.
                "#}));
            }

            #[test]
            fn it_shows_the_gaps_when_quiet() {
                let report = report_with_gap(UnreadablePathSeverity::Error, vec![]);

                let output = render_human(&report, true, FiniteOrUnlimited::Unlimited);

                assert!(output.contains("Files too large to count lines (1):"));
                assert!(!output.contains("Warnings"));
            }

            #[test]
            fn it_lists_the_gaps_before_warnings_under_the_cap() {
                let report = report_with_gap(
                    UnreadablePathSeverity::Error,
                    vec![violation(FileViolationKind::FileLines, "src/lib.rs")],
                );

                let output = render_human(
                    &report,
                    false,
                    FiniteOrUnlimited::Finite(MaxFindings::new(2).unwrap()),
                );

                assert!(output.contains("Warnings (1):\n\n  … and 1 more\n"));
                assert!(output.contains("  src/big.rs: 20 bytes"));
            }

            mod when_the_cap_leaves_one_slot {
                use super::*;

                fn report() -> CheckReport {
                    report_with_gap(
                        UnreadablePathSeverity::Error,
                        vec![violation(FileViolationKind::FileLines, "src/lib.rs")],
                    )
                }

                fn render(format: CheckFormat) -> String {
                    render_check_report(
                        &report(),
                        format,
                        false,
                        FiniteOrUnlimited::Finite(MaxFindings::new(1).unwrap()),
                    )
                    .unwrap()
                }

                #[test]
                fn it_lists_the_gap_that_fails_the_check_instead_of_the_violation() {
                    let output = render(CheckFormat::Human);

                    assert!(output.contains("  src/big.rs: 20 bytes"));
                    assert!(!output.contains("  src/lib.rs:"));
                }

                #[test]
                fn it_annotates_only_the_gap() {
                    let output = render(CheckFormat::Github);

                    assert!(output.contains(
                        "\n::error file=src/big.rs,line=1,title=sprawl-guard/incomplete-analysis::"
                    ));
                    assert!(!output.contains("file=src/lib.rs"));
                }
            }

            #[test]
            fn it_annotates_the_gap_as_an_error_on_its_file() {
                let report = report_with_gap(UnreadablePathSeverity::Error, vec![]);

                let output = render_check_report(
                    &report,
                    CheckFormat::Github,
                    false,
                    FiniteOrUnlimited::Unlimited,
                )
                .unwrap();

                assert!(output.contains(
                    "\n::error file=src/big.rs,line=1,title=sprawl-guard/incomplete-analysis::src/big.rs: 20 bytes"
                ));
            }

            #[test]
            fn it_marks_the_sarif_run_unsuccessful() {
                let report = report_with_gap(UnreadablePathSeverity::Error, vec![]);

                let output: serde_json::Value = serde_json::from_str(
                    &render_check_report(
                        &report,
                        CheckFormat::Sarif,
                        false,
                        FiniteOrUnlimited::Unlimited,
                    )
                    .unwrap(),
                )
                .unwrap();

                assert_eq!(
                    output["runs"][0]["invocations"],
                    json!([{
                        "executionSuccessful": false,
                        "toolExecutionNotifications": [{
                            "level": "error",
                            "message": {
                                "text": "src/big.rs: 20 bytes is over traversal.max_parsed_code_file_bytes (8), so its lines were not counted",
                            },
                            "locations": [{
                                "physicalLocation": {
                                    "artifactLocation": { "uri": "src/big.rs" },
                                    "region": { "startLine": 1 },
                                },
                            }],
                        }],
                    }])
                );
            }
        }

        mod that_are_only_warnings {
            use super::*;

            #[test]
            fn it_hides_the_gaps_when_quiet() {
                let report = report_with_gap(
                    UnreadablePathSeverity::Warning,
                    vec![violation(FileViolationKind::FileLines, "src/lib.rs")],
                );

                let output = render_human(&report, true, FiniteOrUnlimited::Unlimited);

                assert!(!output.contains("Files too large"));
                assert!(!output.contains("The check failed because"));
            }

            #[test]
            fn it_annotates_the_gap_as_a_warning() {
                let report = report_with_gap(UnreadablePathSeverity::Warning, vec![]);

                let output = render_check_report(
                    &report,
                    CheckFormat::Github,
                    false,
                    FiniteOrUnlimited::Unlimited,
                )
                .unwrap();

                assert!(output.contains("\n::warning file=src/big.rs,line=1,"));
            }

            #[test]
            fn it_keeps_the_sarif_run_successful() {
                let report = report_with_gap(UnreadablePathSeverity::Warning, vec![]);

                let output = sarif::sarif_check_report(&report);

                let invocation = &output["runs"][0]["invocations"][0];
                assert_eq!(invocation["executionSuccessful"], true);
                assert_eq!(
                    invocation["toolExecutionNotifications"][0]["level"],
                    "warning"
                );
            }
        }
    }

    mod when_rendering_json_check_output {
        use super::*;

        #[test]
        fn it_uses_the_stable_report_schema() {
            let report = report_with(vec![violation(FileViolationKind::FileLines, "src/lib.rs")]);

            let output = serde_json::to_value(&report).unwrap();

            assert_eq!(
                output,
                json!({
                    "status": "failed",
                    "root": "/repo",
                    "paths": "all",
                    "config_source": { "kind": "built_in" },
                    "coverage": {
                        "kind": "complete"
                    },
                    "summary": {
                        "files_checked": 3,
                        "directories_checked": 2,
                        "languages_detected": 1,
                        "ratchet_entries": 0
                    },
                    "counter": {
                        "backend": "tokei",
                        "version": "14.0.0"
                    },
                    "warnings": [],
                    "violations": [
                        {
                            "kind": "file_lines",
                            "path": "src/lib.rs",
                            "language": "Rust",
                            "count_lines": [],
                            "line_counts": {
                                "code": 3,
                                "comments": 0,
                                "blank_lines": 0
                            },
                            "actual": 3,
                            "limit": 2,
                            "ratchet": null
                        }
                    ]
                })
            );
        }

        #[test]
        fn it_serializes_structured_warnings() {
            let report = report_with_rust_cfg_warning();

            let output = serde_json::to_value(&report).unwrap();

            assert_eq!(
                output["warnings"],
                json!([
                    {
                        "kind": "rust_cfg_test_parse_failed",
                        "path": "src/lib.rs",
                        "detail": "expected identifier"
                    }
                ])
            );
        }
    }
}
