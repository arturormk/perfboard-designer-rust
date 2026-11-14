//! Review notes: findings about a project, usually written by an agent
//! through MCP, that the user deletes as they address them (ADR 0015).
//!
//! Notes are project metadata, like the name: they live outside the design,
//! so they are neither undo steps nor part of commits. Writes act on note IDs
//! rather than on a revision, because notes cannot conflict with design
//! edits, and they leave the revision alone: a note records the revision it
//! was written against, which stays the current one. The session tracks note
//! changes separately so they still count as unsaved work.

use perfboard_core::{ObjectReference, ReviewNoteId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{DocumentRevision, ProjectDocument};

/// Most notes a project keeps.
pub const MAX_REVIEW_NOTES: usize = 500;
/// Longest title, in characters.
pub const MAX_NOTE_TITLE_CHARS: usize = 300;
/// Longest detail text, in characters.
pub const MAX_NOTE_DETAIL_CHARS: usize = 8000;
/// Most object references on one note.
pub const MAX_NOTE_OBJECTS: usize = 32;

/// How much a note matters, most severe first.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReviewNoteSeverity {
    /// Wrong or unsafe as designed; fix before building.
    Critical,
    /// Likely to cause trouble; should be addressed.
    Warning,
    /// An improvement worth considering.
    Suggestion,
    /// Context or an observation; nothing to fix.
    Info,
}

impl ReviewNoteSeverity {
    /// 0 for the most severe; lower ranks are listed first by default.
    #[must_use]
    pub const fn rank(self) -> u8 {
        match self {
            Self::Critical => 0,
            Self::Warning => 1,
            Self::Suggestion => 2,
            Self::Info => 3,
        }
    }
}

/// A stored review note.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ReviewNote {
    pub id: ReviewNoteId,
    pub severity: ReviewNoteSeverity,
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
    /// Design objects the note is about.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<ObjectReference>,
    pub created_ms: u64,
    /// The document revision current when the note was added: the design
    /// it describes. Absent in notes from builds that did not record it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<DocumentRevision>,
}

/// A note as an author submits it; the session assigns its ID and time.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NewReviewNote {
    pub severity: ReviewNoteSeverity,
    /// One-line summary of the finding.
    pub title: String,
    /// Explanation and suggested fix; plain text.
    #[serde(default)]
    pub detail: String,
    /// Design objects the note is about, as `{ "kind", "id" }` with kind
    /// `component`, `wire`, `rail`, `saddle` or `net` and the object's ID
    /// from the design.
    #[serde(default)]
    pub objects: Vec<ObjectReference>,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ReviewNoteError {
    #[error("a review note needs a title")]
    BlankTitle,
    #[error("review note titles are limited to {MAX_NOTE_TITLE_CHARS} characters")]
    TitleTooLong,
    #[error("review note details are limited to {MAX_NOTE_DETAIL_CHARS} characters")]
    DetailTooLong,
    #[error("a review note may reference at most {MAX_NOTE_OBJECTS} objects")]
    TooManyObjects,
    #[error("a project keeps at most {MAX_REVIEW_NOTES} review notes")]
    TooManyNotes,
    #[error("review note {0} was not found")]
    NotFound(ReviewNoteId),
    #[error("review note {0} already exists")]
    DuplicateId(ReviewNoteId),
    #[error("at least one review note is required")]
    Empty,
}

impl NewReviewNote {
    fn into_note(
        self,
        created_ms: u64,
        revision: DocumentRevision,
    ) -> Result<ReviewNote, ReviewNoteError> {
        let note = ReviewNote {
            id: ReviewNoteId::new(),
            severity: self.severity,
            title: self.title,
            detail: self.detail,
            objects: self.objects,
            created_ms,
            revision: Some(revision),
        };
        note.normalized()
    }
}

impl ReviewNote {
    /// Trims the text and checks the limits.
    fn normalized(mut self) -> Result<Self, ReviewNoteError> {
        self.title = self.title.trim().to_owned();
        self.detail = self.detail.trim().to_owned();
        if self.title.is_empty() {
            return Err(ReviewNoteError::BlankTitle);
        }
        if self.title.chars().count() > MAX_NOTE_TITLE_CHARS {
            return Err(ReviewNoteError::TitleTooLong);
        }
        if self.detail.chars().count() > MAX_NOTE_DETAIL_CHARS {
            return Err(ReviewNoteError::DetailTooLong);
        }
        if self.objects.len() > MAX_NOTE_OBJECTS {
            return Err(ReviewNoteError::TooManyObjects);
        }
        Ok(self)
    }
}

impl ProjectDocument {
    /// Adds notes and returns them with their IDs. All or none are added.
    ///
    /// Each note goes in front of the first note that is less severe, or at
    /// the end when there is none, so a list in severity order stays in that
    /// order and notes of equal severity keep the order they arrived in. A
    /// list the user has reordered is not re-sorted: a new critical note
    /// still lands before the first warning, wherever that is.
    ///
    /// # Errors
    /// Returns [`ReviewNoteError`] when a note is invalid or the project would
    /// exceed [`MAX_REVIEW_NOTES`].
    pub fn add_review_notes(
        &mut self,
        notes: Vec<NewReviewNote>,
        created_ms: u64,
    ) -> Result<Vec<ReviewNote>, ReviewNoteError> {
        if notes.is_empty() {
            return Err(ReviewNoteError::Empty);
        }
        if self.review_notes.len() + notes.len() > MAX_REVIEW_NOTES {
            return Err(ReviewNoteError::TooManyNotes);
        }
        let added = notes
            .into_iter()
            .map(|note| note.into_note(created_ms, self.revision))
            .collect::<Result<Vec<_>, _>>()?;
        for note in &added {
            let rank = note.severity.rank();
            let at = self
                .review_notes
                .iter()
                .position(|existing| existing.severity.rank() > rank)
                .unwrap_or(self.review_notes.len());
            self.review_notes.insert(at, note.clone());
        }
        Ok(added)
    }

    /// Removes the notes with these IDs and returns them. All must exist.
    ///
    /// # Errors
    /// Returns [`ReviewNoteError::NotFound`] for the first unknown ID; nothing
    /// is removed then.
    pub fn remove_review_notes(
        &mut self,
        ids: &[ReviewNoteId],
    ) -> Result<Vec<ReviewNote>, ReviewNoteError> {
        if ids.is_empty() {
            return Err(ReviewNoteError::Empty);
        }
        if let Some(missing) = ids
            .iter()
            .find(|id| !self.review_notes.iter().any(|note| note.id == **id))
        {
            return Err(ReviewNoteError::NotFound(*missing));
        }
        let (removed, kept) = std::mem::take(&mut self.review_notes)
            .into_iter()
            .partition(|note| ids.contains(&note.id));
        self.review_notes = kept;
        Ok(removed)
    }

    /// Moves a note in front of `before`, or to the end when `before` is
    /// `None`. Returns whether the order changed.
    ///
    /// # Errors
    /// Returns [`ReviewNoteError::NotFound`] when either note is missing.
    pub fn move_review_note(
        &mut self,
        id: ReviewNoteId,
        before: Option<ReviewNoteId>,
    ) -> Result<bool, ReviewNoteError> {
        let from = self.note_index(id)?;
        if let Some(before) = before {
            self.note_index(before)?;
        }
        if before == Some(id) {
            return Ok(false);
        }
        let note = self.review_notes.remove(from);
        let to = match before {
            Some(before) => self.note_index(before)?,
            None => self.review_notes.len(),
        };
        self.review_notes.insert(to, note);
        Ok(to != from)
    }

    /// Puts back a note that was removed, at `index` (clamped to the end).
    /// Used to undo a deletion in the editor.
    ///
    /// # Errors
    /// Returns [`ReviewNoteError`] when a note with that ID exists, the note
    /// is invalid or the project is full.
    pub fn insert_review_note(
        &mut self,
        note: ReviewNote,
        index: usize,
    ) -> Result<(), ReviewNoteError> {
        if self
            .review_notes
            .iter()
            .any(|existing| existing.id == note.id)
        {
            return Err(ReviewNoteError::DuplicateId(note.id));
        }
        if self.review_notes.len() >= MAX_REVIEW_NOTES {
            return Err(ReviewNoteError::TooManyNotes);
        }
        let note = note.normalized()?;
        let index = index.min(self.review_notes.len());
        self.review_notes.insert(index, note);
        Ok(())
    }

    fn note_index(&self, id: ReviewNoteId) -> Result<usize, ReviewNoteError> {
        self.review_notes
            .iter()
            .position(|note| note.id == id)
            .ok_or(ReviewNoteError::NotFound(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Board;
    use perfboard_core::{BoardSize, ProjectId};

    fn document() -> ProjectDocument {
        ProjectDocument::new(
            ProjectId::new(),
            "Notes",
            Board::new("10x8", BoardSize::new(10, 8).unwrap(), 2.54),
        )
    }

    fn new_note(severity: ReviewNoteSeverity, title: &str) -> NewReviewNote {
        NewReviewNote {
            severity,
            title: title.into(),
            detail: String::new(),
            objects: Vec::new(),
        }
    }

    fn titles(document: &ProjectDocument) -> Vec<&str> {
        document
            .review_notes
            .iter()
            .map(|note| note.title.as_str())
            .collect()
    }

    #[test]
    fn notes_are_appended_in_order_and_record_the_revision() {
        let mut document = document();
        let added = document
            .add_review_notes(
                vec![
                    new_note(ReviewNoteSeverity::Critical, "  A/B swapped "),
                    new_note(ReviewNoteSeverity::Info, "Nice layout"),
                ],
                42,
            )
            .unwrap();
        assert_eq!(added.len(), 2);
        assert_eq!(added[0].title, "A/B swapped");
        assert_eq!(added[0].created_ms, 42);
        assert_eq!(titles(&document), ["A/B swapped", "Nice layout"]);
        assert_eq!(document.revision.0, 0, "notes leave the revision alone");
        assert_eq!(added[0].revision, Some(DocumentRevision(0)));
        assert!(!document.is_dirty(), "notes are not design changes");
    }

    #[test]
    fn new_notes_go_before_the_first_less_severe_note() {
        use ReviewNoteSeverity::{Critical, Info, Suggestion, Warning};
        let mut document = document();
        document
            .add_review_notes(
                vec![
                    new_note(Info, "i1"),
                    new_note(Warning, "w1"),
                    new_note(Suggestion, "s1"),
                    new_note(Warning, "w2"),
                ],
                0,
            )
            .unwrap();
        assert_eq!(titles(&document), ["w1", "w2", "s1", "i1"]);
        document
            .add_review_notes(vec![new_note(Critical, "c1"), new_note(Warning, "w3")], 0)
            .unwrap();
        assert_eq!(titles(&document), ["c1", "w1", "w2", "w3", "s1", "i1"]);
    }

    #[test]
    fn a_reordered_list_is_not_resorted() {
        use ReviewNoteSeverity::{Critical, Info, Warning};
        let mut document = document();
        let added = document
            .add_review_notes(vec![new_note(Critical, "c1"), new_note(Info, "i1")], 0)
            .unwrap();
        document
            .move_review_note(added[1].id, Some(added[0].id))
            .unwrap();
        assert_eq!(titles(&document), ["i1", "c1"]);
        document
            .add_review_notes(vec![new_note(Critical, "c2"), new_note(Warning, "w1")], 0)
            .unwrap();
        assert_eq!(titles(&document), ["c2", "w1", "i1", "c1"]);
    }

    #[test]
    fn invalid_notes_add_nothing() {
        let mut document = document();
        let error = document
            .add_review_notes(
                vec![
                    new_note(ReviewNoteSeverity::Warning, "Fine"),
                    new_note(ReviewNoteSeverity::Warning, "   "),
                ],
                0,
            )
            .unwrap_err();
        assert_eq!(error, ReviewNoteError::BlankTitle);
        assert_eq!(document.review_notes, Vec::new());
        assert_eq!(document.revision.0, 0);
    }

    #[test]
    fn notes_move_before_another_or_to_the_end() {
        let mut document = document();
        let ids: Vec<_> = document
            .add_review_notes(
                ["a", "b", "c"]
                    .map(|title| new_note(ReviewNoteSeverity::Suggestion, title))
                    .into(),
                0,
            )
            .unwrap()
            .iter()
            .map(|note| note.id)
            .collect();
        assert!(document.move_review_note(ids[2], Some(ids[0])).unwrap());
        assert_eq!(titles(&document), ["c", "a", "b"]);
        assert!(document.move_review_note(ids[2], None).unwrap());
        assert_eq!(titles(&document), ["a", "b", "c"]);
        assert!(!document.move_review_note(ids[0], Some(ids[1])).unwrap());
        assert!(!document.move_review_note(ids[1], Some(ids[1])).unwrap());
        let unknown = ReviewNoteId::new();
        assert_eq!(
            document.move_review_note(unknown, None),
            Err(ReviewNoteError::NotFound(unknown))
        );
        assert_eq!(
            document.move_review_note(ids[0], Some(unknown)),
            Err(ReviewNoteError::NotFound(unknown))
        );
        assert_eq!(titles(&document), ["a", "b", "c"]);
    }

    #[test]
    fn removed_notes_can_be_put_back_where_they_were() {
        let mut document = document();
        let added = document
            .add_review_notes(
                ["a", "b", "c"]
                    .map(|title| new_note(ReviewNoteSeverity::Warning, title))
                    .into(),
                0,
            )
            .unwrap();
        let removed = document.remove_review_notes(&[added[1].id]).unwrap();
        assert_eq!(titles(&document), ["a", "c"]);
        document.insert_review_note(removed[0].clone(), 1).unwrap();
        assert_eq!(titles(&document), ["a", "b", "c"]);
        assert_eq!(
            document.insert_review_note(removed[0].clone(), 0),
            Err(ReviewNoteError::DuplicateId(added[1].id))
        );
        let unknown = ReviewNoteId::new();
        assert_eq!(
            document.remove_review_notes(&[added[0].id, unknown]),
            Err(ReviewNoteError::NotFound(unknown))
        );
        assert_eq!(document.review_notes.len(), 3);
    }
}
