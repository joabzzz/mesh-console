//! Undo/redo + revision tracking for the configuration editor, mirroring
//! `useConfigurationHistory.ts` in the web console. Pure data structure over
//! JSON config snapshots; the UI layers tab-specific edits on top.

use serde_json::Value;

#[derive(Clone, Debug)]
pub struct ConfigHistory {
    /// Server revision the editing session is based on (compare-and-swap
    /// expectation for apply).
    base_revision: u64,
    /// The config as loaded from the server.
    baseline: Value,
    /// Undo stack: `snapshots[cursor]` is the current state.
    snapshots: Vec<Value>,
    cursor: usize,
}

impl ConfigHistory {
    pub fn new(base_revision: u64, config: Value) -> Self {
        Self {
            base_revision,
            baseline: config.clone(),
            snapshots: vec![config],
            cursor: 0,
        }
    }

    pub fn base_revision(&self) -> u64 {
        self.base_revision
    }

    pub fn current(&self) -> &Value {
        &self.snapshots[self.cursor]
    }

    pub fn is_dirty(&self) -> bool {
        *self.current() != self.baseline
    }

    pub fn can_undo(&self) -> bool {
        self.cursor > 0
    }

    pub fn can_redo(&self) -> bool {
        self.cursor + 1 < self.snapshots.len()
    }

    /// Push a new edited state; truncates any redo branch. No-op when the
    /// edit equals the current state.
    pub fn push(&mut self, next: Value) {
        if next == *self.current() {
            return;
        }
        self.snapshots.truncate(self.cursor + 1);
        self.snapshots.push(next);
        self.cursor += 1;
    }

    pub fn undo(&mut self) -> Option<&Value> {
        if !self.can_undo() {
            return None;
        }
        self.cursor -= 1;
        Some(self.current())
    }

    pub fn redo(&mut self) -> Option<&Value> {
        if !self.can_redo() {
            return None;
        }
        self.cursor += 1;
        Some(self.current())
    }

    /// Discard edits back to the loaded baseline.
    pub fn reset_to_baseline(&mut self) {
        let baseline = self.baseline.clone();
        self.push(baseline);
    }

    /// After a successful apply: the applied state becomes the new baseline
    /// at the server's new revision.
    pub fn mark_applied(&mut self, new_revision: u64) {
        self.base_revision = new_revision;
        self.baseline = self.current().clone();
    }

    /// After a revision conflict: reload the server's snapshot but keep the
    /// user's edited state on top of it so nothing they typed is lost. The
    /// new server config becomes both baseline and an undo point.
    pub fn rebase(&mut self, server_revision: u64, server_config: Value) {
        let edited = self.current().clone();
        self.base_revision = server_revision;
        self.baseline = server_config.clone();
        self.snapshots = vec![server_config];
        self.cursor = 0;
        self.push(edited);
    }
}

/// Read a nested value by dotted path (arrays not supported — config tabs
/// edit object fields).
pub fn get_path<'v>(root: &'v Value, path: &str) -> Option<&'v Value> {
    let mut node = root;
    for key in path.split('.') {
        node = node.as_object()?.get(key)?;
    }
    Some(node)
}

/// Set a nested value by dotted path, creating intermediate objects. Returns
/// a modified clone (snapshots are immutable history entries).
pub fn with_path_set(root: &Value, path: &str, value: Value) -> Value {
    let mut next = root.clone();
    let mut node = &mut next;
    let keys: Vec<&str> = path.split('.').collect();
    for key in &keys[..keys.len() - 1] {
        if !node.is_object() {
            *node = Value::Object(serde_json::Map::new());
        }
        node = node
            .as_object_mut()
            .expect("just ensured object")
            .entry(key.to_string())
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
    }
    if !node.is_object() {
        *node = Value::Object(serde_json::Map::new());
    }
    node.as_object_mut()
        .expect("just ensured object")
        .insert(keys[keys.len() - 1].to_string(), value);
    next
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn undo_redo_and_dirty_tracking() {
        let mut h = ConfigHistory::new(3, json!({"a": 1}));
        assert!(!h.is_dirty() && !h.can_undo() && !h.can_redo());

        h.push(json!({"a": 2}));
        assert!(h.is_dirty() && h.can_undo());

        h.undo();
        assert_eq!(h.current(), &json!({"a": 1}));
        assert!(!h.is_dirty() && h.can_redo());

        h.redo();
        assert_eq!(h.current(), &json!({"a": 2}));

        // New edit truncates the redo branch.
        h.undo();
        h.push(json!({"a": 3}));
        assert!(!h.can_redo());
        assert_eq!(h.current(), &json!({"a": 3}));
    }

    #[test]
    fn duplicate_push_is_a_no_op() {
        let mut h = ConfigHistory::new(1, json!({"a": 1}));
        h.push(json!({"a": 1}));
        assert!(!h.can_undo());
    }

    #[test]
    fn apply_resets_baseline_and_revision() {
        let mut h = ConfigHistory::new(1, json!({"a": 1}));
        h.push(json!({"a": 2}));
        h.mark_applied(2);
        assert_eq!(h.base_revision(), 2);
        assert!(!h.is_dirty());
        // Undo history survives the apply (can still step back).
        assert!(h.can_undo());
    }

    #[test]
    fn rebase_keeps_user_edits_on_new_baseline() {
        let mut h = ConfigHistory::new(1, json!({"a": 1}));
        h.push(json!({"a": 2, "mine": true}));
        h.rebase(5, json!({"a": 9}));
        assert_eq!(h.base_revision(), 5);
        assert_eq!(h.current(), &json!({"a": 2, "mine": true}));
        assert!(h.is_dirty());
        h.undo();
        assert_eq!(h.current(), &json!({"a": 9}), "undo reaches server state");
    }

    #[test]
    fn path_helpers_get_and_set_nested_fields() {
        let root = json!({"defaults": {"model_fit": {"ctx_size": 4096}}});
        assert_eq!(
            get_path(&root, "defaults.model_fit.ctx_size"),
            Some(&json!(4096))
        );
        assert_eq!(get_path(&root, "defaults.missing"), None);

        let next = with_path_set(&root, "defaults.model_fit.ctx_size", json!(8192));
        assert_eq!(
            get_path(&next, "defaults.model_fit.ctx_size"),
            Some(&json!(8192))
        );
        // Creates intermediate objects.
        let next = with_path_set(&root, "gpu.parallel", json!(4));
        assert_eq!(get_path(&next, "gpu.parallel"), Some(&json!(4)));
        // Original untouched.
        assert_eq!(
            get_path(&root, "defaults.model_fit.ctx_size"),
            Some(&json!(4096))
        );
    }
}
