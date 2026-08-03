use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Deserializer, Serialize};

use crate::{CustomLanguageGroupId, LanguageGroupId, LanguageId, LeafFileLimit};

use super::MaxLeafFilesPerDirectoryLimits;
use super::limits::{
    PartialMaxLeafFilesPerDirectoryLimits, merge_max_leaf_files_per_directory_limits,
};

/// Error returned when a custom language group's members are invalid.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum LanguageGroupMembersError {
    /// The group did not contain any languages.
    #[error("language group must contain at least one language")]
    Empty,
    /// The group listed one language more than once.
    #[error("language group contains duplicate language {language}")]
    Duplicate {
        /// Repeated language.
        language: LanguageId,
    },
}

/// Non-empty, duplicate-free languages assigned to a custom group.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct LanguageGroupMembers(Vec<LanguageId>);

impl LanguageGroupMembers {
    /// Returns the configured languages in authored order.
    pub fn as_slice(&self) -> &[LanguageId] {
        &self.0
    }
}

impl TryFrom<Vec<LanguageId>> for LanguageGroupMembers {
    type Error = LanguageGroupMembersError;

    fn try_from(languages: Vec<LanguageId>) -> Result<Self, Self::Error> {
        if languages.is_empty() {
            return Err(LanguageGroupMembersError::Empty);
        }

        let mut seen = BTreeSet::new();
        for language in &languages {
            if !seen.insert(*language) {
                return Err(LanguageGroupMembersError::Duplicate {
                    language: *language,
                });
            }
        }

        Ok(Self(languages))
    }
}

impl<'de> Deserialize<'de> for LanguageGroupMembers {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Vec::<LanguageId>::deserialize(deserializer)?
            .try_into()
            .map_err(serde::de::Error::custom)
    }
}

/// Directory fan-out limits for one language group.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LanguageGroupLimits {
    /// Maximum direct leaf source files per directory.
    pub max_leaf_files_per_directory: MaxLeafFilesPerDirectoryLimits,
}

/// Resolved configuration for one custom language group.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CustomLanguageGroupConfig {
    /// Languages assigned to the group.
    pub languages: LanguageGroupMembers,
    /// Limits applied to the group.
    pub limits: LanguageGroupLimits,
}

/// Resolved configuration for the automatic documentation group.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentationLanguageGroupConfig {
    /// Limits applied to documentation languages.
    pub limits: LanguageGroupLimits,
}

/// Fully resolved language-group configuration and language assignments.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct LanguageGroupsConfig {
    /// Automatic documentation-language configuration.
    pub(crate) documentation: DocumentationLanguageGroupConfig,
    /// User-defined groups keyed by their validated names.
    #[serde(flatten)]
    pub(crate) custom: BTreeMap<CustomLanguageGroupId, CustomLanguageGroupConfig>,
    #[serde(skip)]
    assignments: BTreeMap<LanguageId, LanguageGroupId>,
}

impl LanguageGroupsConfig {
    /// Creates the automatic assignments used when no groups are authored.
    pub(crate) fn automatic(languages: impl IntoIterator<Item = LanguageId>) -> Self {
        Self {
            documentation: DocumentationLanguageGroupConfig {
                limits: LanguageGroupLimits {
                    max_leaf_files_per_directory: unlimited_directory_limits(),
                },
            },
            custom: BTreeMap::new(),
            assignments: languages
                .into_iter()
                .map(|language| (language, language.automatic_group()))
                .collect(),
        }
    }

    /// Returns the resolved group for a supported language.
    pub fn group_for(&self, language: LanguageId) -> &LanguageGroupId {
        self.assignments
            .get(&language)
            .expect("resolved config must assign every bundled language")
    }

    /// Returns the automatic documentation-group configuration.
    pub fn documentation(&self) -> &DocumentationLanguageGroupConfig {
        &self.documentation
    }

    /// Returns custom groups keyed by their validated names.
    pub fn custom(&self) -> &BTreeMap<CustomLanguageGroupId, CustomLanguageGroupConfig> {
        &self.custom
    }
}

/// Error returned when language-group sections conflict or omit required fields.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum LanguageGroupsError {
    /// The automatic source group was authored directly.
    #[error("source group is configured through global limits")]
    AuthoredSource,
    /// Documentation membership was authored even though it is automatic.
    #[error("documentation group membership is automatic")]
    DocumentationLanguages,
    /// A custom group omitted its required language list.
    #[error("custom language group {group} must declare languages")]
    MissingCustomLanguages {
        /// Custom group missing its members.
        group: CustomLanguageGroupId,
    },
    /// Two custom groups claimed the same language.
    #[error("language {language} belongs to both {first_group} and {second_group}")]
    DuplicateAssignment {
        /// Language assigned twice.
        language: LanguageId,
        /// First group containing the language.
        first_group: CustomLanguageGroupId,
        /// Second group containing the language.
        second_group: CustomLanguageGroupId,
    },
}

#[derive(Debug, Deserialize)]
#[serde(transparent)]
pub(super) struct PartialLanguageGroupsConfig(
    BTreeMap<LanguageGroupId, PartialLanguageGroupConfig>,
);

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PartialLanguageGroupConfig {
    languages: Option<LanguageGroupMembers>,
    limits: Option<PartialLanguageGroupLimits>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PartialLanguageGroupLimits {
    max_leaf_files_per_directory: Option<PartialMaxLeafFilesPerDirectoryLimits>,
}

pub(super) fn resolve_language_groups(
    partial: Option<PartialLanguageGroupsConfig>,
    languages: impl IntoIterator<Item = LanguageId>,
    global_limits: &MaxLeafFilesPerDirectoryLimits,
) -> Result<LanguageGroupsConfig, LanguageGroupsError> {
    let mut resolved = LanguageGroupsConfig::automatic(languages);
    let Some(PartialLanguageGroupsConfig(groups)) = partial else {
        return Ok(resolved);
    };
    let mut custom_assignments = BTreeMap::<LanguageId, CustomLanguageGroupId>::new();

    for (group_id, partial_group) in groups {
        match group_id {
            LanguageGroupId::Source => return Err(LanguageGroupsError::AuthoredSource),
            LanguageGroupId::Documentation => {
                if partial_group.languages.is_some() {
                    return Err(LanguageGroupsError::DocumentationLanguages);
                }
                apply_group_limits(
                    &mut resolved.documentation.limits.max_leaf_files_per_directory,
                    partial_group.limits,
                );
            }
            LanguageGroupId::Custom(group) => {
                let languages = partial_group.languages.ok_or_else(|| {
                    LanguageGroupsError::MissingCustomLanguages {
                        group: group.clone(),
                    }
                })?;
                for language in languages.as_slice() {
                    if let Some(first_group) = custom_assignments.insert(*language, group.clone()) {
                        return Err(LanguageGroupsError::DuplicateAssignment {
                            language: *language,
                            first_group,
                            second_group: group,
                        });
                    }
                    resolved
                        .assignments
                        .insert(*language, LanguageGroupId::Custom(group.clone()));
                }

                let mut directory_limits = global_limits.clone();
                apply_group_limits(&mut directory_limits, partial_group.limits);
                resolved.custom.insert(
                    group,
                    CustomLanguageGroupConfig {
                        languages,
                        limits: LanguageGroupLimits {
                            max_leaf_files_per_directory: directory_limits,
                        },
                    },
                );
            }
        }
    }

    Ok(resolved)
}

fn apply_group_limits(
    resolved: &mut MaxLeafFilesPerDirectoryLimits,
    partial: Option<PartialLanguageGroupLimits>,
) {
    let Some(PartialLanguageGroupLimits {
        max_leaf_files_per_directory: Some(partial),
    }) = partial
    else {
        return;
    };
    merge_max_leaf_files_per_directory_limits(resolved, partial);
}

fn unlimited_directory_limits() -> MaxLeafFilesPerDirectoryLimits {
    MaxLeafFilesPerDirectoryLimits {
        production: LeafFileLimit::Unlimited,
        test: LeafFileLimit::Unlimited,
    }
}
