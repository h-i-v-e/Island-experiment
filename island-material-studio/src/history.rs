//! Bounded snapshot history for the material-studio document.
//!
//! The history deliberately stores complete recipe snapshots.  Recipes are
//! small relative to generated texture maps, and a complete snapshot keeps
//! undo semantics correct when a field edit changes a cross-field invariant or
//! a layer operation changes several references at once.

use std::collections::VecDeque;

/// The default number of committed transactions retained by a document.
pub const DEFAULT_HISTORY_LIMIT: usize = 100;

/// One edit transaction, represented by its state before and after the edit.
#[derive(Clone, Debug, PartialEq)]
pub struct EditTransaction<T> {
    /// State before the transaction began.
    pub before: T,
    /// State after the transaction was committed.
    pub after: T,
}

/// A bounded undo/redo stack with an explicit gesture transaction.
///
/// `begin_gesture` captures one snapshot.  The owner may then mutate its live
/// value as often as needed and calls `commit_gesture` once when the gesture
/// ends.  This is the intended path for slider drags and text edits that emit
/// many intermediate UI events.
#[derive(Clone, Debug)]
pub struct History<T> {
    undo: VecDeque<EditTransaction<T>>,
    redo: Vec<EditTransaction<T>>,
    limit: usize,
    gesture_before: Option<T>,
}

impl<T> Default for History<T> {
    fn default() -> Self {
        Self::new(DEFAULT_HISTORY_LIMIT)
    }
}

impl<T> History<T> {
    /// Creates an empty history with a bounded transaction capacity.
    #[must_use]
    pub fn new(limit: usize) -> Self {
        Self {
            undo: VecDeque::new(),
            redo: Vec::new(),
            limit,
            gesture_before: None,
        }
    }

    /// Returns the maximum number of undo transactions retained.
    #[must_use]
    pub const fn limit(&self) -> usize {
        self.limit
    }

    /// Changes the capacity and drops the oldest undo transactions if needed.
    ///
    /// Redo entries are cleared because the meaning of a redo branch is no
    /// longer useful after changing the history policy.
    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit;
        while self.undo.len() > limit {
            self.undo.pop_front();
        }
        self.redo.clear();
    }

    /// Returns the number of undoable transactions.
    #[must_use]
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    /// Returns the number of redoable transactions.
    #[must_use]
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    /// Whether at least one transaction can be undone.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether at least one transaction can be redone.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Whether a gesture is currently open.
    #[must_use]
    pub fn gesture_active(&self) -> bool {
        self.gesture_before.is_some()
    }

    /// Borrows the current undo entries from oldest to newest.
    #[must_use]
    pub fn undo_entries(&self) -> impl DoubleEndedIterator<Item = &EditTransaction<T>> {
        self.undo.iter()
    }

    /// Clears both stacks and any in-progress gesture.
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.gesture_before = None;
    }
}

impl<T: Clone + PartialEq> History<T> {
    /// Records one complete transaction.
    ///
    /// Returns `false` when the snapshots are equal.  Recording a new edit
    /// always clears the redo branch.  If a gesture is active, callers should
    /// use [`Self::commit_gesture`] instead; this method remains usable and
    /// records the supplied transaction directly.
    pub fn record(&mut self, before: T, after: T) -> bool {
        if before == after {
            return false;
        }
        self.gesture_before = None;
        self.redo.clear();
        if self.limit != 0 {
            if self.undo.len() == self.limit {
                self.undo.pop_front();
            }
            self.undo.push_back(EditTransaction { before, after });
        }
        true
    }

    /// Starts one gesture from the supplied current snapshot.
    ///
    /// Returns `false` when a gesture is already active.  A caller should
    /// finish or cancel the existing gesture before starting another one.
    pub fn begin_gesture(&mut self, current: &T) -> bool {
        if self.gesture_before.is_some() {
            return false;
        }
        self.gesture_before = Some(current.clone());
        true
    }

    /// Commits the active gesture against the supplied final snapshot.
    ///
    /// Equal before/after snapshots are ignored, so a click that does not
    /// change a value does not consume an undo slot.  The returned transaction
    /// is useful to owners that need to update a revision only when a change
    /// occurred.
    pub fn commit_gesture(&mut self, current: &T) -> Option<EditTransaction<T>> {
        let before = self.gesture_before.take()?;
        if before == *current {
            return None;
        }
        self.redo.clear();
        let transaction = EditTransaction {
            before,
            after: current.clone(),
        };
        if self.limit != 0 {
            if self.undo.len() == self.limit {
                self.undo.pop_front();
            }
            self.undo.push_back(transaction.clone());
        }
        Some(transaction)
    }

    /// Cancels the active gesture and returns its starting snapshot.
    pub fn cancel_gesture(&mut self) -> Option<T> {
        self.gesture_before.take()
    }

    /// Moves the newest undo transaction to redo and returns its before state.
    ///
    /// The caller supplies the live current state so redo can restore the
    /// exact transaction branch even if the value type has additional state
    /// outside the history owner.
    pub fn undo(&mut self, current: &T) -> Option<T> {
        self.gesture_before = None;
        let transaction = self.undo.pop_back()?;
        self.redo.push(transaction.clone());
        // A normal history owner is at `transaction.after`.  Returning the
        // stored before snapshot is intentional even if `current` differs;
        // the owner then becomes deterministic rather than attempting an
        // inverse operation against a possibly stale value.
        let _ = current;
        Some(transaction.before)
    }

    /// Moves the newest redo transaction to undo and returns its after state.
    pub fn redo(&mut self, current: &T) -> Option<T> {
        self.gesture_before = None;
        let transaction = self.redo.pop()?;
        self.undo.push_back(transaction.clone());
        if self.undo.len() > self.limit {
            self.undo.pop_front();
        }
        let _ = current;
        Some(transaction.after)
    }
}

/// Name used by callers that prefer to spell out the snapshot nature of the
/// implementation.
pub type SnapshotHistory<T> = History<T>;

#[cfg(test)]
mod tests {
    use super::{DEFAULT_HISTORY_LIMIT, History};

    #[test]
    fn gesture_coalesces_many_intermediate_values() {
        let mut history = History::new(8);
        let mut current = 0;
        assert!(history.begin_gesture(&current));
        current = 1;
        assert_eq!(current, 1);
        current = 2;
        assert_eq!(current, 2);
        current = 3;
        let transaction = history
            .commit_gesture(&current)
            .expect("gesture changed value");
        assert_eq!(transaction.before, 0);
        assert_eq!(transaction.after, 3);
        assert_eq!(history.undo_len(), 1);
        assert_eq!(history.undo(&current), Some(0));
        assert_eq!(history.redo(&0), Some(3));
    }

    #[test]
    fn equal_gesture_does_not_consume_history() {
        let mut history = History::default();
        assert!(history.begin_gesture(&String::from("same")));
        assert!(history.commit_gesture(&String::from("same")).is_none());
        assert!(!history.can_undo());
    }

    #[test]
    fn history_is_bounded_and_new_edits_clear_redo() {
        let mut history = History::new(2);
        assert!(history.record(0, 1));
        assert!(history.record(1, 2));
        assert!(history.record(2, 3));
        assert_eq!(history.undo_len(), 2);
        assert_eq!(history.undo(&3), Some(2));
        assert_eq!(history.redo_len(), 1);
        assert!(history.record(2, 9));
        assert_eq!(history.redo_len(), 0);
        assert_eq!(history.undo_len(), 2);
    }

    #[test]
    fn default_limit_matches_document_contract() {
        assert_eq!(History::<u8>::default().limit(), DEFAULT_HISTORY_LIMIT);
    }
}
