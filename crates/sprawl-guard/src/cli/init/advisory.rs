use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::num::NonZeroUsize;
use std::path::Path;

use serde::Serialize;
use serde_json::json;
use sprawl_guard_lib::check_without_ratchet;
use sprawl_guard_lib::classification::ClassifiedSource;
use sprawl_guard_lib::{
    CheckOptions, CheckViolation, DirectoryViolationKind, FileViolationKind, LanguageId,
};

use crate::error::CliError;
use crate::error::Result;

use super::init_language_key;

const GENERATED_VENDOR_ADVISORY_THRESHOLD: usize = 100;

pub(super) struct InitAdvisory {
    source_summary: SourceSummary,
    report: sprawl_guard_lib::CheckReport,
}

pub(super) fn run_advisory_scan(
    root: &Path,
    target: &Path,
    config: &sprawl_guard_lib::config::Config,
) -> Result<InitAdvisory> {
    run_advisory_scan_inner(root, config).map_err(|source| CliError::InitAdvisoryScan {
        path: target.to_path_buf(),
        source: Box::new(source),
    })
}

pub(super) fn write_advisory_summary(output: &mut String, advisory: &InitAdvisory) {
    advisory.source_summary.write_summary(output);
    let counts = ViolationCounts::from_report(&advisory.report);
    if counts.is_empty() {
        output.push_str("Current tree has no violations under the new policy.\n");
    } else {
        output.push_str("Current tree has violations under the new policy:\n");
        writeln!(
            output,
            "  production file LOC violations: {}",
            counts.production_file_loc
        )
        .unwrap();
        writeln!(
            output,
            "  test file LOC violations: {}",
            counts.test_file_loc
        )
        .unwrap();
        writeln!(
            output,
            "  production directory fan-out violations: {}",
            counts.production_directory_fanout
        )
        .unwrap();
        writeln!(
            output,
            "  test directory fan-out violations: {}",
            counts.test_directory_fanout.message()
        )
        .unwrap();
    }
    advisory
        .source_summary
        .write_generated_vendor_advisories(output);
}

pub(super) fn advisory_json(advisory: &InitAdvisory) -> serde_json::Value {
    json!({
        "source_summary": advisory.source_summary,
        "violation_counts": ViolationCounts::from_report(&advisory.report),
    })
}

fn run_advisory_scan_inner(
    root: &Path,
    config: &sprawl_guard_lib::config::Config,
) -> Result<InitAdvisory> {
    let sources = sprawl_guard_lib::classification::discover_sources(root, config)?;
    let source_summary = SourceSummary::from_sources(&sources);
    let report = check_without_ratchet(CheckOptions {
        root: root.to_path_buf(),
        config,
        paths: vec![],
        require_ratchet_current: Some(false),
        traversal_error_policy: None,
    })?;
    Ok(InitAdvisory {
        source_summary,
        report,
    })
}

#[derive(Default, Serialize)]
struct SourceSummary {
    language_counts: BTreeMap<LanguageId, usize>,
    generated_vendor_warnings: Vec<GeneratedVendorWarning>,
}

impl SourceSummary {
    fn from_sources(sources: &[ClassifiedSource]) -> Self {
        let mut language_counts = BTreeMap::new();
        let mut generated_vendor_counts: BTreeMap<
            GeneratedVendorSegment,
            BTreeMap<LanguageId, usize>,
        > = BTreeMap::new();
        for source in sources {
            *language_counts.entry(source.language()).or_insert(0) += 1;
            if let Some(segment) = generated_vendor_segment(source) {
                *generated_vendor_counts
                    .entry(segment.to_owned())
                    .or_default()
                    .entry(source.language())
                    .or_insert(0) += 1;
            }
        }
        let generated_vendor_warnings = generated_vendor_counts
            .into_iter()
            .filter_map(|(segment, language_counts)| {
                GeneratedVendorWarning::new(segment, language_counts)
            })
            .collect();
        Self {
            language_counts,
            generated_vendor_warnings,
        }
    }

    fn write_summary(&self, output: &mut String) {
        if self.language_counts.is_empty() {
            return;
        }
        output.push_str("Included source files:\n");
        for (language, count) in &self.language_counts {
            writeln!(
                output,
                "  {:<12} {}",
                init_language_key(*language),
                file_count(*count)
            )
            .unwrap();
        }
        output.push('\n');
    }

    fn write_generated_vendor_advisories(&self, output: &mut String) {
        if self.generated_vendor_warnings.is_empty() {
            return;
        }
        output.push('\n');
        output.push_str("Large generated/vendor-like directories were included:\n");
        for warning in &self.generated_vendor_warnings {
            writeln!(
                output,
                "  {}/          {}",
                warning.segment.as_str(),
                warning.language_summary()
            )
            .unwrap();
        }
        output.push('\n');
        output.push_str("sprawl-guard has no built-in semantic excludes.\n");
        output.push_str(
            "Add [files].exclude entries or ignore rules if these should not be checked.\n",
        );
    }
}

fn generated_vendor_segment(source: &ClassifiedSource) -> Option<GeneratedVendorSegment> {
    source
        .path
        .as_str()
        .split('/')
        .find_map(GeneratedVendorSegment::from_path_segment)
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
enum GeneratedVendorSegment {
    Dist,
    Build,
    Target,
    Out,
    Generated,
    Vendor,
    NodeModules,
    Next,
    Turbo,
}

impl GeneratedVendorSegment {
    fn from_path_segment(segment: &str) -> Option<Self> {
        match segment {
            "dist" => Some(Self::Dist),
            "build" => Some(Self::Build),
            "target" => Some(Self::Target),
            "out" => Some(Self::Out),
            "generated" => Some(Self::Generated),
            "vendor" => Some(Self::Vendor),
            "node_modules" => Some(Self::NodeModules),
            ".next" => Some(Self::Next),
            ".turbo" => Some(Self::Turbo),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Dist => "dist",
            Self::Build => "build",
            Self::Target => "target",
            Self::Out => "out",
            Self::Generated => "generated",
            Self::Vendor => "vendor",
            Self::NodeModules => "node_modules",
            Self::Next => ".next",
            Self::Turbo => ".turbo",
        }
    }
}

#[derive(Serialize)]
struct GeneratedVendorWarning {
    segment: GeneratedVendorSegment,
    language_counts: BTreeMap<LanguageId, usize>,
}

impl GeneratedVendorWarning {
    fn new(
        segment: GeneratedVendorSegment,
        language_counts: BTreeMap<LanguageId, usize>,
    ) -> Option<Self> {
        let total = language_counts.values().sum::<usize>();
        (total >= GENERATED_VENDOR_ADVISORY_THRESHOLD).then_some(Self {
            segment,
            language_counts,
        })
    }

    fn language_summary(&self) -> String {
        self.language_counts
            .iter()
            .map(|(language, count)| {
                format!("{} {}", file_count(*count), init_language_key(*language))
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[derive(Default, Serialize)]
struct ViolationCounts {
    production_file_loc: usize,
    test_file_loc: usize,
    production_directory_fanout: usize,
    test_directory_fanout: TestDirectoryFanoutViolations,
}

impl ViolationCounts {
    fn from_report(report: &sprawl_guard_lib::CheckReport) -> Self {
        let mut counts = Self::default();
        for violation in report.violations() {
            match violation {
                CheckViolation::FileLoc(violation) => match violation.kind {
                    FileViolationKind::FileLoc => counts.production_file_loc += 1,
                    FileViolationKind::TestFileLoc => counts.test_file_loc += 1,
                },
                CheckViolation::DirectoryFanout(violation) => match violation.kind {
                    DirectoryViolationKind::DirectoryFanout => {
                        counts.production_directory_fanout += 1;
                    }
                    DirectoryViolationKind::TestDirectoryFanout => {
                        counts.test_directory_fanout.increment();
                    }
                },
                CheckViolation::Ratchet(_) => {}
            }
        }
        counts
    }

    fn is_empty(&self) -> bool {
        self.production_file_loc == 0
            && self.test_file_loc == 0
            && self.production_directory_fanout == 0
            && self.test_directory_fanout.is_empty()
    }
}

#[derive(Default, Serialize)]
#[serde(rename_all = "snake_case")]
enum TestDirectoryFanoutViolations {
    #[default]
    NotEnforced,
    Counted(NonZeroUsize),
}

impl TestDirectoryFanoutViolations {
    fn increment(&mut self) {
        match self {
            Self::NotEnforced => *self = Self::Counted(NonZeroUsize::MIN),
            Self::Counted(count) => {
                *count =
                    NonZeroUsize::new(count.get() + 1).expect("incremented count must be nonzero");
            }
        }
    }

    fn is_empty(&self) -> bool {
        match self {
            Self::NotEnforced => true,
            Self::Counted(_) => false,
        }
    }

    fn message(&self) -> String {
        match self {
            Self::NotEnforced => "not enforced (unlimited)".to_owned(),
            Self::Counted(count) => count.to_string(),
        }
    }
}

fn file_count(count: usize) -> String {
    match count {
        1 => "1 file".to_owned(),
        count => format!("{count} files"),
    }
}
