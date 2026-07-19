use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PendingTextAction {
    pub action_id: String,
    pub submitted_draft: String,
}

fn store() -> Option<web_sys::Storage> {
    crate::window().local_storage().ok()?
}

fn draft_key(pane_id: &str) -> String {
    format!("kelpie:draft:{pane_id}")
}

fn pending_key(pane_id: &str) -> String {
    format!("kelpie:pending-text:{pane_id}")
}

pub fn load_draft(pane_id: &str) -> String {
    store()
        .and_then(|store| store.get_item(&draft_key(pane_id)).ok().flatten())
        .unwrap_or_default()
}

pub fn save_draft(pane_id: &str, value: &str) {
    if let Some(store) = store() {
        let _ = store.set_item(&draft_key(pane_id), value);
    }
}

pub fn clear_draft_if_matches(pane_id: &str, expected: &str) {
    let Some(store) = store() else {
        return;
    };
    let key = draft_key(pane_id);
    let current = store.get_item(&key).ok().flatten();
    if current.as_deref() != Some(expected) && !(current.is_none() && expected.is_empty()) {
        return;
    }
    let _ = store.remove_item(&key);
}

pub fn load_pending_text(pane_id: &str) -> Option<PendingTextAction> {
    let raw = store()?.get_item(&pending_key(pane_id)).ok()??;
    serde_json::from_str(&raw).ok()
}

pub fn save_pending_text(pane_id: &str, pending: &PendingTextAction) -> bool {
    let (Some(store), Ok(raw)) = (store(), serde_json::to_string(pending)) else {
        return false;
    };
    store.set_item(&pending_key(pane_id), &raw).is_ok()
}

pub fn clear_pending_text(pane_id: &str, action_id: &str) {
    let Some(store) = store() else {
        return;
    };
    let key = pending_key(pane_id);
    let matches = store
        .get_item(&key)
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str::<PendingTextAction>(&raw).ok())
        .is_some_and(|pending| pending.action_id == action_id);
    if matches {
        let _ = store.remove_item(&key);
    }
}
