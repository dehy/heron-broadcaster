// SPDX-License-Identifier: AGPL-3.0-or-later

//! Library work that `heron-api` asks of `heron-librarian` through the
//! `library_tasks` table (the API cannot write files), and its results.

use serde::{Deserialize, Serialize};

/// A task, as stored in `library_tasks.params`. The tag is also its `kind`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TaskRequest {
    /// Dry run of an organization: lists the moves, changes nothing.
    OrganizePlan,
    /// Carries out the moves of a dry run, as one batch.
    OrganizeApply { plan_task_id: u64 },
    /// Reverts a batch.
    Undo { batch_id: u64 },
    /// Applies a release picked on the review screen to a track: its tags
    /// are written and it is filed where the layout says, as one batch.
    ReviewApply {
        track_id: u64,
        release_id: String,
        recording_id: String,
        disc: Option<u32>,
        track: Option<u32>,
    },
    /// Looks up the MusicBrainz relationships between the station's
    /// artists ("member of", "is person", …) and suggests the links.
    ArtistRelations,
}

impl TaskRequest {
    /// The value of `library_tasks.kind`.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::OrganizePlan => "organize_plan",
            Self::OrganizeApply { .. } => "organize_apply",
            Self::Undo { .. } => "undo",
            Self::ReviewApply { .. } => "review_apply",
            Self::ArtistRelations => "artist_relations",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PlannedMove {
    pub track_id: u64,
    /// Relative to the library root.
    pub from: String,
    pub to: String,
}

/// The dry run of an organization.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct OrganizePlan {
    pub moves: Vec<PlannedMove>,
    /// Tracks already where the layout puts them.
    pub unchanged: u64,
    /// Tracks whose file cannot be read (left where they are).
    pub unreadable: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SkippedFile {
    pub path: String,
    pub reason: String,
}

/// What a batch of file operations did.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct BatchReport {
    pub batch_id: u64,
    pub done: u64,
    /// Operations not done, with the reason.
    pub skipped: Vec<SkippedFile>,
}

/// What a MusicBrainz relationships lookup found.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RelationsReport {
    /// Artists with a MusicBrainz ID looked up.
    pub artists: u64,
    /// New suggestions, between two artists of the station.
    pub suggestions: u64,
    /// Lookups that failed (tried again next time).
    pub failed: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_are_tagged_by_kind() {
        let request = TaskRequest::Undo { batch_id: 4 };
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json, serde_json::json!({"kind": "undo", "batch_id": 4}));
        assert_eq!(json["kind"], request.kind());
        let plan: TaskRequest = serde_json::from_str(r#"{"kind": "organize_plan"}"#).unwrap();
        assert_eq!(plan, TaskRequest::OrganizePlan);
    }
}
