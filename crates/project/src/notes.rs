//! Review notes: findings about a project, usually written by an agent
//! through MCP, that the user deletes as they address them (ADR 0015).
//!
//! The user can also answer a note: acknowledge it, or reply and request a
//! review, which the agent answers with a response the next time it is asked
//! to look at the notes (ADR 0016).
//!
//! The user can keep notes worth holding on to in a second list, Issues
//! (ADR 0020). Agents read Issues but cannot change them: every agent write
//! names [`NoteList::Notes`], and only the editor can name
//! [`NoteList::Issues`] or move notes between the lists.
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

/// Most notes a project keeps in each list.
pub const MAX_REVIEW_NOTES: usize = 500;
/// Longest title, in characters.
pub const MAX_NOTE_TITLE_CHARS: usize = 300;
/// Longest detail text, in characters.
pub const MAX_NOTE_DETAIL_CHARS: usize = 8000;
/// Most object references on one note.
pub const MAX_NOTE_OBJECTS: usize = 32;
/// Longest reply or response, in characters.
pub const MAX_NOTE_REPLY_CHARS: usize = 2000;

/// The two lists a note can be in (ADR 0020).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NoteList {
    /// The agent's working list: it adds, answers, reorders and removes
    /// notes here.
    #[default]
    Notes,
    /// Notes the user keeps across reviews. Agents can read them; only the
    /// user changes them.
    Issues,
}

impl NoteList {
    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::Notes => Self::Issues,
            Self::Issues => Self::Notes,
        }
    }
}

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

/// Where a note stands in the exchange between the user and the agent.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReviewNoteStatus {
    /// Not yet answered by the user, or answered again by the agent.
    #[default]
    Open,
    /// The user accepts the finding and will deal with it.
    Acknowledged,
    /// The user disagrees or has a question (see `reply`) and wants the
    /// agent to review the note again.
    ReviewRequested,
}

impl ReviewNoteStatus {
    // serde's `skip_serializing_if` passes a reference.
    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn is_open(&self) -> bool {
        *self == Self::Open
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
    #[serde(default, skip_serializing_if = "ReviewNoteStatus::is_open")]
    pub status: ReviewNoteStatus,
    /// The user's answer to the note; plain text.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reply: String,
    /// The agent's answer to the reply, after reviewing the note again.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub response: String,
}

/// Changes to a stored note. Absent fields are left as they are; an empty
/// `reply` or `response` clears it.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ReviewNoteUpdate {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<ReviewNoteSeverity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<ReviewNoteStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<String>,
}

impl ReviewNoteUpdate {
    fn is_empty(&self) -> bool {
        *self == Self::default()
    }
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

#[derive(Clone, Debug, Error, Eq, PartialEq)]
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
    #[error("a project keeps at most {MAX_REVIEW_NOTES} issues")]
    TooManyIssues,
    #[error("review note {0} was not found")]
    NotFound(ReviewNoteId),
    #[error("review note {0} is in the user's Issues list; only the user can change or move it")]
    InIssues(ReviewNoteId),
    #[error("review note {0} already exists")]
    DuplicateId(ReviewNoteId),
    #[error("at least one review note is required")]
    Empty,
    #[error("review note replies and responses are limited to {MAX_NOTE_REPLY_CHARS} characters")]
    ReplyTooLong,
    #[error("a review note update must change at least one field")]
    EmptyUpdate,
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
            status: ReviewNoteStatus::Open,
            reply: String::new(),
            response: String::new(),
        };
        note.normalized()
    }
}

impl ReviewNote {
    /// Trims the text and checks the limits.
    fn normalized(mut self) -> Result<Self, ReviewNoteError> {
        self.title = self.title.trim().to_owned();
        self.detail = self.detail.trim().to_owned();
        self.reply = self.reply.trim().to_owned();
        self.response = self.response.trim().to_owned();
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
        if self.reply.chars().count() > MAX_NOTE_REPLY_CHARS
            || self.response.chars().count() > MAX_NOTE_REPLY_CHARS
        {
            return Err(ReviewNoteError::ReplyTooLong);
        }
        Ok(self)
    }
}

impl ProjectDocument {
    /// The notes in `list`, in the user's order.
    #[must_use]
    pub fn note_list(&self, list: NoteList) -> &[ReviewNote] {
        match list {
            NoteList::Notes => &self.review_notes,
            NoteList::Issues => &self.issues,
        }
    }

    fn note_list_mut(&mut self, list: NoteList) -> &mut Vec<ReviewNote> {
        match list {
            NoteList::Notes => &mut self.review_notes,
            NoteList::Issues => &mut self.issues,
        }
    }

    /// The list holding the note with this ID, if any.
    #[must_use]
    pub fn list_of_note(&self, id: ReviewNoteId) -> Option<NoteList> {
        [NoteList::Notes, NoteList::Issues]
            .into_iter()
            .find(|list| self.note_list(*list).iter().any(|note| note.id == id))
    }

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

    /// Removes the notes with these IDs from `list` and returns them. All
    /// must be in that list.
    ///
    /// # Errors
    /// Returns [`ReviewNoteError::NotFound`] (or
    /// [`ReviewNoteError::InIssues`]) for the first ID not in `list`; nothing
    /// is removed then.
    pub fn remove_review_notes(
        &mut self,
        list: NoteList,
        ids: &[ReviewNoteId],
    ) -> Result<Vec<ReviewNote>, ReviewNoteError> {
        if ids.is_empty() {
            return Err(ReviewNoteError::Empty);
        }
        for id in ids {
            self.note_index(list, *id)?;
        }
        let (removed, kept) = std::mem::take(self.note_list_mut(list))
            .into_iter()
            .partition(|note| ids.contains(&note.id));
        *self.note_list_mut(list) = kept;
        Ok(removed)
    }

    /// Moves a note within `list` in front of `before`, or to the end when
    /// `before` is `None`. Returns whether the order changed.
    ///
    /// # Errors
    /// Returns [`ReviewNoteError::NotFound`] (or
    /// [`ReviewNoteError::InIssues`]) when either note is not in `list`.
    pub fn move_review_note(
        &mut self,
        list: NoteList,
        id: ReviewNoteId,
        before: Option<ReviewNoteId>,
    ) -> Result<bool, ReviewNoteError> {
        let from = self.note_index(list, id)?;
        if let Some(before) = before {
            self.note_index(list, before)?;
        }
        if before == Some(id) {
            return Ok(false);
        }
        let note = self.note_list_mut(list).remove(from);
        let to = match before {
            Some(before) => self.note_index(list, before)?,
            None => self.note_list(list).len(),
        };
        self.note_list_mut(list).insert(to, note);
        Ok(to != from)
    }

    /// Moves a note into list `to`, in front of `before` or at the end, from
    /// whichever list holds it. The note itself is unchanged: its status,
    /// reply, response and revision go with it. Returns whether anything
    /// moved.
    ///
    /// # Errors
    /// Returns [`ReviewNoteError`] when the note or `before` is missing, or
    /// the target list is full.
    pub fn transfer_review_note(
        &mut self,
        id: ReviewNoteId,
        to: NoteList,
        before: Option<ReviewNoteId>,
    ) -> Result<bool, ReviewNoteError> {
        let from = self.list_of_note(id).ok_or(ReviewNoteError::NotFound(id))?;
        if from == to {
            return self.move_review_note(to, id, before);
        }
        if let Some(before) = before {
            self.note_index(to, before)?;
        }
        if self.note_list(to).len() >= MAX_REVIEW_NOTES {
            return Err(match to {
                NoteList::Notes => ReviewNoteError::TooManyNotes,
                NoteList::Issues => ReviewNoteError::TooManyIssues,
            });
        }
        let index = self.note_index(from, id)?;
        let note = self.note_list_mut(from).remove(index);
        let at = match before {
            Some(before) => self.note_index(to, before)?,
            None => self.note_list(to).len(),
        };
        self.note_list_mut(to).insert(at, note);
        Ok(true)
    }

    /// Puts back a note that was removed, into `list` at `index` (clamped
    /// to the end). Used to undo a deletion in the editor.
    ///
    /// # Errors
    /// Returns [`ReviewNoteError`] when a note with that ID exists in either
    /// list, the note is invalid or the list is full.
    pub fn insert_review_note(
        &mut self,
        list: NoteList,
        note: ReviewNote,
        index: usize,
    ) -> Result<(), ReviewNoteError> {
        if self.list_of_note(note.id).is_some() {
            return Err(ReviewNoteError::DuplicateId(note.id));
        }
        if self.note_list(list).len() >= MAX_REVIEW_NOTES {
            return Err(match list {
                NoteList::Notes => ReviewNoteError::TooManyNotes,
                NoteList::Issues => ReviewNoteError::TooManyIssues,
            });
        }
        let note = note.normalized()?;
        let notes = self.note_list_mut(list);
        let index = index.min(notes.len());
        notes.insert(index, note);
        Ok(())
    }

    /// Applies `update` to a note in the Notes list and returns the note as it now is, plus
    /// whether anything changed. The note is left alone when the result
    /// would be invalid.
    ///
    /// Changing the severity, title, detail or response is the author
    /// reviewing the note against the current design, so the note then
    /// records the current revision. The user's status and reply leave it.
    ///
    /// # Errors
    /// Returns [`ReviewNoteError`] when the note is missing, the update is
    /// empty or the result breaks a limit.
    pub fn update_review_note(
        &mut self,
        id: ReviewNoteId,
        update: ReviewNoteUpdate,
    ) -> Result<(ReviewNote, bool), ReviewNoteError> {
        if update.is_empty() {
            return Err(ReviewNoteError::EmptyUpdate);
        }
        let index = self.note_index(NoteList::Notes, id)?;
        let current = &self.review_notes[index];
        let mut next = current.clone();
        let reviewed = update.severity.is_some()
            || update.title.is_some()
            || update.detail.is_some()
            || update.response.is_some();
        if let Some(severity) = update.severity {
            next.severity = severity;
        }
        if let Some(title) = update.title {
            next.title = title;
        }
        if let Some(detail) = update.detail {
            next.detail = detail;
        }
        if let Some(status) = update.status {
            next.status = status;
        }
        if let Some(reply) = update.reply {
            next.reply = reply;
        }
        if let Some(response) = update.response {
            next.response = response;
        }
        let mut next = next.normalized()?;
        if reviewed && next != *current {
            next.revision = Some(self.revision);
        }
        let changed = next != *current;
        self.review_notes[index] = next.clone();
        Ok((next, changed))
    }

    fn note_index(&self, list: NoteList, id: ReviewNoteId) -> Result<usize, ReviewNoteError> {
        self.note_list(list)
            .iter()
            .position(|note| note.id == id)
            .ok_or_else(|| {
                if list == NoteList::Notes && self.list_of_note(id) == Some(NoteList::Issues) {
                    ReviewNoteError::InIssues(id)
                } else {
                    ReviewNoteError::NotFound(id)
                }
            })
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
            .move_review_note(NoteList::Notes, added[1].id, Some(added[0].id))
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
        assert!(
            document
                .move_review_note(NoteList::Notes, ids[2], Some(ids[0]))
                .unwrap()
        );
        assert_eq!(titles(&document), ["c", "a", "b"]);
        assert!(
            document
                .move_review_note(NoteList::Notes, ids[2], None)
                .unwrap()
        );
        assert_eq!(titles(&document), ["a", "b", "c"]);
        assert!(
            !document
                .move_review_note(NoteList::Notes, ids[0], Some(ids[1]))
                .unwrap()
        );
        assert!(
            !document
                .move_review_note(NoteList::Notes, ids[1], Some(ids[1]))
                .unwrap()
        );
        let unknown = ReviewNoteId::new();
        assert_eq!(
            document.move_review_note(NoteList::Notes, unknown, None),
            Err(ReviewNoteError::NotFound(unknown))
        );
        assert_eq!(
            document.move_review_note(NoteList::Notes, ids[0], Some(unknown)),
            Err(ReviewNoteError::NotFound(unknown))
        );
        assert_eq!(titles(&document), ["a", "b", "c"]);
    }

    #[test]
    fn notes_take_a_status_reply_and_response() {
        let mut document = document();
        let id = document
            .add_review_notes(vec![new_note(ReviewNoteSeverity::Warning, "Bulk cap")], 0)
            .unwrap()[0]
            .id;
        let (note, changed) = document
            .update_review_note(
                id,
                ReviewNoteUpdate {
                    status: Some(ReviewNoteStatus::ReviewRequested),
                    reply: Some("  The module has one already. ".into()),
                    ..ReviewNoteUpdate::default()
                },
            )
            .unwrap();
        assert!(changed);
        assert_eq!(note.status, ReviewNoteStatus::ReviewRequested);
        assert_eq!(note.reply, "The module has one already.");
        assert_eq!(note.title, "Bulk cap", "absent fields are kept");

        let (note, _) = document
            .update_review_note(
                id,
                ReviewNoteUpdate {
                    severity: Some(ReviewNoteSeverity::Info),
                    status: Some(ReviewNoteStatus::Open),
                    response: Some("Agreed; the module's 10 µF is enough.".into()),
                    ..ReviewNoteUpdate::default()
                },
            )
            .unwrap();
        assert_eq!(note.severity, ReviewNoteSeverity::Info);
        assert_eq!(note.reply, "The module has one already.");
        assert_eq!(document.review_notes[0], note);

        let (_, changed) = document
            .update_review_note(
                id,
                ReviewNoteUpdate {
                    status: Some(ReviewNoteStatus::Open),
                    ..ReviewNoteUpdate::default()
                },
            )
            .unwrap();
        assert!(!changed);
    }

    #[test]
    fn invalid_updates_leave_the_note_alone() {
        let mut document = document();
        let id = document
            .add_review_notes(vec![new_note(ReviewNoteSeverity::Warning, "Keep")], 0)
            .unwrap()[0]
            .id;
        let before = document.review_notes.clone();
        assert_eq!(
            document.update_review_note(id, ReviewNoteUpdate::default()),
            Err(ReviewNoteError::EmptyUpdate)
        );
        assert_eq!(
            document.update_review_note(
                id,
                ReviewNoteUpdate {
                    status: Some(ReviewNoteStatus::Acknowledged),
                    reply: Some("x".repeat(MAX_NOTE_REPLY_CHARS + 1)),
                    ..ReviewNoteUpdate::default()
                }
            ),
            Err(ReviewNoteError::ReplyTooLong)
        );
        assert_eq!(
            document.update_review_note(
                id,
                ReviewNoteUpdate {
                    title: Some(" ".into()),
                    ..ReviewNoteUpdate::default()
                }
            ),
            Err(ReviewNoteError::BlankTitle)
        );
        assert_eq!(document.review_notes, before);
    }

    #[test]
    fn open_notes_serialize_without_status_reply_or_response() {
        let mut document = document();
        document
            .add_review_notes(vec![new_note(ReviewNoteSeverity::Info, "Plain")], 0)
            .unwrap();
        let json = serde_json::to_value(&document.review_notes[0]).unwrap();
        assert!(json.get("status").is_none());
        assert!(json.get("reply").is_none());
        assert!(json.get("response").is_none());
        let id = document.review_notes[0].id;
        document
            .update_review_note(
                id,
                ReviewNoteUpdate {
                    status: Some(ReviewNoteStatus::ReviewRequested),
                    ..ReviewNoteUpdate::default()
                },
            )
            .unwrap();
        let json = serde_json::to_value(&document.review_notes[0]).unwrap();
        assert_eq!(json["status"], "review_requested");
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
        let removed = document
            .remove_review_notes(NoteList::Notes, &[added[1].id])
            .unwrap();
        assert_eq!(titles(&document), ["a", "c"]);
        document
            .insert_review_note(NoteList::Notes, removed[0].clone(), 1)
            .unwrap();
        assert_eq!(titles(&document), ["a", "b", "c"]);
        assert_eq!(
            document.insert_review_note(NoteList::Notes, removed[0].clone(), 0),
            Err(ReviewNoteError::DuplicateId(added[1].id))
        );
        let unknown = ReviewNoteId::new();
        assert_eq!(
            document.remove_review_notes(NoteList::Notes, &[added[0].id, unknown]),
            Err(ReviewNoteError::NotFound(unknown))
        );
        assert_eq!(document.review_notes.len(), 3);
    }
    #[test]
    fn notes_move_to_issues_and_back_unchanged() {
        let mut document = document();
        let added = document
            .add_review_notes(
                ["a", "b", "c"]
                    .map(|title| new_note(ReviewNoteSeverity::Warning, title))
                    .into(),
                0,
            )
            .unwrap();
        document
            .update_review_note(
                added[1].id,
                ReviewNoteUpdate {
                    status: Some(ReviewNoteStatus::Acknowledged),
                    reply: Some("Will add it".into()),
                    ..ReviewNoteUpdate::default()
                },
            )
            .unwrap();
        let kept = document.review_notes[1].clone();
        assert!(
            document
                .transfer_review_note(added[1].id, NoteList::Issues, None)
                .unwrap()
        );
        assert_eq!(titles(&document), ["a", "c"]);
        assert_eq!(document.issues, std::slice::from_ref(&kept));
        assert_eq!(document.list_of_note(kept.id), Some(NoteList::Issues));
        assert!(
            document
                .transfer_review_note(added[0].id, NoteList::Issues, Some(kept.id))
                .unwrap()
        );
        assert_eq!(document.issues[0].title, "a");
        assert!(
            document
                .transfer_review_note(kept.id, NoteList::Notes, Some(added[2].id))
                .unwrap()
        );
        assert_eq!(titles(&document), ["b", "c"]);
        assert_eq!(document.review_notes[0], kept);
        assert_eq!(document.revision.0, 0);
    }

    #[test]
    fn notes_writes_cannot_touch_issues() {
        let mut document = document();
        let added = document
            .add_review_notes(
                ["a", "b"]
                    .map(|title| new_note(ReviewNoteSeverity::Info, title))
                    .into(),
                0,
            )
            .unwrap();
        let issue = added[0].id;
        document
            .transfer_review_note(issue, NoteList::Issues, None)
            .unwrap();
        let before = document.issues.clone();
        let blocked = ReviewNoteError::InIssues(issue);
        assert_eq!(
            document.remove_review_notes(NoteList::Notes, &[issue]),
            Err(blocked.clone())
        );
        assert_eq!(
            document.move_review_note(NoteList::Notes, issue, None),
            Err(blocked.clone())
        );
        assert_eq!(
            document.move_review_note(NoteList::Notes, added[1].id, Some(issue)),
            Err(blocked.clone())
        );
        assert_eq!(
            document
                .update_review_note(
                    issue,
                    ReviewNoteUpdate {
                        response: Some("x".into()),
                        ..ReviewNoteUpdate::default()
                    }
                )
                .map(|_| ()),
            Err(blocked)
        );
        assert_eq!(document.issues, before);
        assert_eq!(titles(&document), ["b"]);
        let removed = document
            .remove_review_notes(NoteList::Issues, &[issue])
            .unwrap();
        assert_eq!(
            document.insert_review_note(NoteList::Notes, removed[0].clone(), 0),
            Ok(())
        );
        assert_eq!(
            document.insert_review_note(NoteList::Issues, removed[0].clone(), 0),
            Err(ReviewNoteError::DuplicateId(issue))
        );
    }
}
