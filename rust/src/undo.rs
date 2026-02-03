//! Undo/redo history for terrain modifications.
//!
//! Stores snapshots of `Arc<ModificationLayer>` for efficient undo/redo.
//! Since the modification layer uses `Arc`, pushing a snapshot is just a
//! reference count bump — the actual data is shared until a new commit
//! creates a modified clone.

use std::sync::Arc;

use crate::terrain_modifications::ModificationLayer;

/// Snapshot-based undo/redo history for terrain modifications.
pub struct UndoHistory {
    /// Stack of previous states (most recent at the end)
    past: Vec<Arc<ModificationLayer>>,
    /// Stack of undone states available for redo (most recent at the end)
    future: Vec<Arc<ModificationLayer>>,
    /// Maximum number of undo entries to keep
    max_entries: usize,
}

impl UndoHistory {
    pub fn new(max_entries: usize) -> Self {
        Self {
            past: Vec::new(),
            future: Vec::new(),
            max_entries: max_entries.max(1),
        }
    }

    /// Push the current state before a new modification.
    /// Clears the redo stack (new action invalidates redo).
    pub fn push(&mut self, state: Arc<ModificationLayer>) {
        self.future.clear();
        self.past.push(state);
        // Trim oldest entries if over capacity
        while self.past.len() > self.max_entries {
            self.past.remove(0);
        }
    }

    /// Undo: pop the most recent past state, push current to future.
    /// Returns the state to restore, or None if nothing to undo.
    pub fn undo(&mut self, current: Arc<ModificationLayer>) -> Option<Arc<ModificationLayer>> {
        let previous = self.past.pop()?;
        self.future.push(current);
        Some(previous)
    }

    /// Redo: pop the most recent future state, push current to past.
    /// Returns the state to restore, or None if nothing to redo.
    pub fn redo(&mut self, current: Arc<ModificationLayer>) -> Option<Arc<ModificationLayer>> {
        let next = self.future.pop()?;
        self.past.push(current);
        Some(next)
    }

    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    pub fn clear(&mut self) {
        self.past.clear();
        self.future.clear();
    }

    pub fn undo_count(&self) -> usize {
        self.past.len()
    }

    pub fn redo_count(&self) -> usize {
        self.future.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_layer() -> Arc<ModificationLayer> {
        Arc::new(ModificationLayer::new(32, 1.0))
    }

    #[test]
    fn test_new_history_empty() {
        let history = UndoHistory::new(10);
        assert!(!history.can_undo());
        assert!(!history.can_redo());
        assert_eq!(history.undo_count(), 0);
        assert_eq!(history.redo_count(), 0);
    }

    #[test]
    fn test_push_and_undo() {
        let mut history = UndoHistory::new(10);
        let state_a = make_layer();
        let state_b = make_layer();

        // Push state A (the state before modification)
        history.push(Arc::clone(&state_a));
        assert!(history.can_undo());
        assert!(!history.can_redo());

        // Undo: should return state A, current (state B) goes to future
        let restored = history.undo(Arc::clone(&state_b));
        assert!(restored.is_some());
        assert!(!history.can_undo());
        assert!(history.can_redo());
    }

    #[test]
    fn test_undo_redo_cycle() {
        let mut history = UndoHistory::new(10);
        let state_a = make_layer();
        let state_b = make_layer();
        let state_c = make_layer();

        // state_a -> modify -> state_b -> modify -> state_c
        history.push(Arc::clone(&state_a));
        history.push(Arc::clone(&state_b));

        // Undo from state_c: get state_b
        let restored = history.undo(Arc::clone(&state_c)).unwrap();
        assert!(Arc::ptr_eq(&restored, &state_b));
        assert!(history.can_undo());
        assert!(history.can_redo());

        // Redo from state_b: get state_c
        let restored = history.redo(Arc::clone(&state_b)).unwrap();
        assert!(Arc::ptr_eq(&restored, &state_c));
        assert!(history.can_undo()); // state_a and state_b in past
        assert!(!history.can_redo());
    }

    #[test]
    fn test_new_action_clears_redo() {
        let mut history = UndoHistory::new(10);
        let state_a = make_layer();
        let state_b = make_layer();
        let state_c = make_layer();

        history.push(Arc::clone(&state_a));
        history.push(Arc::clone(&state_b));

        // Undo once
        history.undo(Arc::clone(&state_c));
        assert!(history.can_redo());

        // New action should clear redo
        let state_d = make_layer();
        history.push(Arc::clone(&state_d));
        assert!(!history.can_redo());
    }

    #[test]
    fn test_max_entries_trim() {
        let mut history = UndoHistory::new(3);

        for _ in 0..5 {
            history.push(make_layer());
        }

        assert_eq!(history.undo_count(), 3);
    }

    #[test]
    fn test_clear() {
        let mut history = UndoHistory::new(10);
        history.push(make_layer());
        history.push(make_layer());

        history.clear();
        assert!(!history.can_undo());
        assert!(!history.can_redo());
    }

    #[test]
    fn test_undo_empty_returns_none() {
        let mut history = UndoHistory::new(10);
        assert!(history.undo(make_layer()).is_none());
    }

    #[test]
    fn test_redo_empty_returns_none() {
        let mut history = UndoHistory::new(10);
        assert!(history.redo(make_layer()).is_none());
    }
}
