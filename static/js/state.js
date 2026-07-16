import { api } from './api.js';

// ------------------------------------------------------------
// Fleet cache — shared pane/workspace/tab lookup for every view
// ------------------------------------------------------------
const paneIndex = new Map();
const workspaceIndex = new Map();
const tabsIndex = new Map(); // workspace_id -> [{tab_id, workspace_id, label, pane_ids}]

function paneStatus(pane) {
  return (pane && (pane.agent_status || pane.status)) || 'unknown';
}

function buildTabsIndex(data) {
  if (data && Array.isArray(data.tabs) && data.tabs.length > 0) {
    for (const t of data.tabs) {
      if (!t || t.workspace_id === undefined) continue;
      const list = tabsIndex.get(t.workspace_id) || [];
      list.push(t);
      tabsIndex.set(t.workspace_id, list);
    }
    return;
  }
  // Fallback (pre-v2 bridge): derive minimal tabs by grouping panes on
  // their existing tab_id field, preserving encounter order.
  const groups = new Map();
  for (const p of (data && data.panes) || []) {
    if (!p || p.tab_id === undefined) continue;
    let g = groups.get(p.tab_id);
    if (!g) {
      g = { tab_id: p.tab_id, workspace_id: p.workspace_id, label: null, pane_ids: [] };
      groups.set(p.tab_id, g);
    }
    g.pane_ids.push(p.pane_id);
  }
  for (const g of groups.values()) {
    const list = tabsIndex.get(g.workspace_id) || [];
    list.push(g);
    tabsIndex.set(g.workspace_id, list);
  }
}

async function refreshFleet() {
  const data = await api.fleet();
  paneIndex.clear();
  workspaceIndex.clear();
  tabsIndex.clear();
  for (const ws of (data && data.workspaces) || []) {
    if (ws && ws.id !== undefined) workspaceIndex.set(ws.id, ws.label || ws.id);
  }
  for (const p of (data && data.panes) || []) {
    if (p && p.pane_id !== undefined) paneIndex.set(p.pane_id, p);
  }
  buildTabsIndex(data);
  return data;
}

function isTabActive(tab, pane) {
  if (!pane || !tab) return false;
  if (pane.tab_id !== undefined && tab.tab_id !== undefined) return pane.tab_id === tab.tab_id;
  return Array.isArray(tab.pane_ids) && tab.pane_ids.includes(pane.pane_id);
}

function navigateToTab(tab) {
  const firstPaneId = tab.pane_ids && tab.pane_ids[0];
  if (!firstPaneId) return;
  const pane = paneIndex.get(firstPaneId);
  if (pane && pane.agent) location.hash = '#/pane/' + encodeURIComponent(firstPaneId);
  else location.hash = '#/term/' + encodeURIComponent(firstPaneId);
}

export { paneIndex, workspaceIndex, tabsIndex, paneStatus, refreshFleet, isTabActive, navigateToTab };
