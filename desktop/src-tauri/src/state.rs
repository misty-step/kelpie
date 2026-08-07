//! Shared app state: fleet snapshot + bounded projection cache.

use crate::omp::Projection;
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Max session projections kept in memory. Each live projection holds the
/// full transcript of a session (can exceed 100 MB on very long sessions).
pub const PROJECTION_CACHE_CAP: usize = 6;

#[derive(Clone, Debug, Serialize)]
pub struct Workspace {
    pub id: String,
    pub label: String,
    pub status: String,
    pub active_tab_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Pane {
    pub pane_id: String,
    pub workspace_id: String,
    pub status: String,
    pub task: Option<String>,
    pub pending_ask: bool,
    pub snippet: Option<String>,
    /// Last-write time of the session file in epoch millis, for "5m ago"
    /// style recency in the sidebar.
    pub updated_ms: Option<u64>,
    /// Working directory reported by herdr (used for workspace command discovery).
    pub cwd: Option<String>,
    /// Internal control-plane path; never serialized to the frontend.
    #[serde(skip_serializing)]
    pub session_path: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Fleet {
    pub workspaces: Vec<Workspace>,
    pub panes: Vec<Pane>,
}

#[derive(Default)]
struct ProjectionCache {
    map: HashMap<String, Arc<Mutex<Projection>>>,
    order: VecDeque<String>,
}

impl ProjectionCache {
    fn get_or_create(&mut self, path: &str) -> Arc<Mutex<Projection>> {
        if let Some(projection) = self.map.get(path) {
            return projection.clone();
        }
        if self.map.len() >= PROJECTION_CACHE_CAP {
            if let Some(oldest) = self.order.pop_front() {
                self.map.remove(&oldest);
            }
        }
        let projection = Arc::new(Mutex::new(Projection::new()));
        self.map.insert(path.to_string(), projection.clone());
        self.order.push_back(path.to_string());
        projection
    }

    fn touch(&mut self, path: &str) {
        if let Some(pos) = self.order.iter().position(|p| p == path) {
            if let Some(p) = self.order.remove(pos) {
                self.order.push_back(p);
            }
        }
    }
}

#[derive(Default)]
pub struct AppState {
    pub fleet: Mutex<Option<Fleet>>,
    /// Serializes TUI-driving writes per pane (ask picker navigation).
    pub pane_locks: Mutex<HashSet<String>>,
    cache: Mutex<ProjectionCache>,
}

impl AppState {
    pub async fn projection_for(&self, path: &str) -> Arc<Mutex<Projection>> {
        let mut cache = self.cache.lock().await;
        let projection = cache.get_or_create(path);
        cache.touch(path);
        projection
    }
}
