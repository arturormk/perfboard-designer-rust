//! Asks before the window closes over unsaved work.
//!
//! Closing the window with unsaved project changes (including review notes,
//! which agents write without the user touching the editor) or an unsaved
//! part draft is intercepted, and the editor asks whether to save. The editor
//! acknowledges the request when it shows its dialog. If it never does, for
//! example because the page has hung, a second close request after
//! [`UNRESPONSIVE_AFTER`] goes through, so the window can always be closed.

use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

use serde::Serialize;

/// How long an unacknowledged close request waits before another close
/// request is allowed through.
pub const UNRESPONSIVE_AFTER: Duration = Duration::from_secs(3);

/// The event asking the editor to confirm closing.
pub const CLOSE_REQUESTED_EVENT: &str = "close-requested";

/// What would be lost, sent with [`CLOSE_REQUESTED_EVENT`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct UnsavedWork {
    pub project: bool,
    pub part_draft: bool,
}

impl UnsavedWork {
    #[must_use]
    pub const fn any(&self) -> bool {
        self.project || self.part_draft
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum Decision {
    Close,
    AskEditor,
}

#[derive(Debug, Default)]
struct Pending {
    requested_at: Option<Instant>,
    acknowledged: bool,
}

#[derive(Debug, Default)]
pub struct CloseGuard(Mutex<Pending>);

impl CloseGuard {
    /// Decides what a close request does.
    pub fn on_close_requested(&self, unsaved: &UnsavedWork, now: Instant) -> Decision {
        if !unsaved.any() {
            return Decision::Close;
        }
        let mut pending = self.lock();
        match pending.requested_at {
            Some(at) if !pending.acknowledged && now.duration_since(at) >= UNRESPONSIVE_AFTER => {
                Decision::Close
            }
            Some(_) => Decision::AskEditor,
            None => {
                *pending = Pending {
                    requested_at: Some(now),
                    acknowledged: false,
                };
                Decision::AskEditor
            }
        }
    }

    /// The editor is showing its dialog; wait for the user however long.
    pub fn acknowledge(&self) {
        self.lock().acknowledged = true;
    }

    /// The dialog was answered.
    pub fn reset(&self) {
        *self.lock() = Pending::default();
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Pending> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNSAVED: UnsavedWork = UnsavedWork {
        project: true,
        part_draft: false,
    };

    #[test]
    fn saved_work_closes_at_once() {
        let guard = CloseGuard::default();
        let saved = UnsavedWork {
            project: false,
            part_draft: false,
        };
        assert_eq!(
            guard.on_close_requested(&saved, Instant::now()),
            Decision::Close
        );
    }

    #[test]
    fn unsaved_work_asks_and_waits_for_an_acknowledged_dialog() {
        let guard = CloseGuard::default();
        let start = Instant::now();
        assert_eq!(
            guard.on_close_requested(&UNSAVED, start),
            Decision::AskEditor
        );
        guard.acknowledge();
        let later = start + UNRESPONSIVE_AFTER * 10;
        assert_eq!(
            guard.on_close_requested(&UNSAVED, later),
            Decision::AskEditor
        );
        guard.reset();
        assert_eq!(
            guard.on_close_requested(&UNSAVED, later),
            Decision::AskEditor
        );
    }

    #[test]
    fn an_unresponsive_editor_cannot_keep_the_window_open() {
        let guard = CloseGuard::default();
        let start = Instant::now();
        let draft = UnsavedWork {
            project: false,
            part_draft: true,
        };
        assert_eq!(guard.on_close_requested(&draft, start), Decision::AskEditor);
        assert_eq!(
            guard.on_close_requested(&draft, start + Duration::from_secs(1)),
            Decision::AskEditor
        );
        assert_eq!(
            guard.on_close_requested(&draft, start + UNRESPONSIVE_AFTER),
            Decision::Close
        );
    }
}
