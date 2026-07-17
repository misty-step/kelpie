// ------------------------------------------------------------
// API layer — every call defensive, never throws past caller
// ------------------------------------------------------------
const api = {
  async fleet() {
    const r = await fetch('/api/fleet');
    if (!r.ok) throw new Error('fleet fetch failed: ' + r.status);
    return r.json();
  },
  async session(paneId) {
    const r = await fetch('/api/session/' + encodeURIComponent(paneId));
    if (!r.ok) throw new Error('session fetch failed: ' + r.status);
    return r.json();
  },
  async screen(paneId) {
    const r = await fetch('/api/pane/' + encodeURIComponent(paneId) + '/screen');
    if (!r.ok) {
      const e = new Error('screen fetch failed: ' + r.status);
      e.status = r.status;
      throw e;
    }
    return r.json();
  },
  async commands() {
    const r = await fetch('/api/commands');
    if (!r.ok) throw new Error('commands fetch failed: ' + r.status);
    return r.json();
  },
  async sendText(paneId, text) {
    const r = await fetch('/api/pane/' + encodeURIComponent(paneId) + '/text', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ text }),
    });
    if (!r.ok) throw new Error('send failed: ' + r.status);
    return r.json();
  },
  async upload(paneId, file) {
    const r = await fetch('/api/pane/' + encodeURIComponent(paneId) + '/upload', {
      method: 'POST',
      headers: { 'Content-Type': file.type || 'application/octet-stream' },
      body: file,
    });
    if (!r.ok) throw new Error('upload failed: ' + r.status);
    return r.json();
  },
  async sendAsk(paneId, index) {
    const r = await fetch('/api/pane/' + encodeURIComponent(paneId) + '/ask', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ index }),
    });
    if (!r.ok) throw new Error('ask failed: ' + r.status);
    return r.json();
  },
  async sendKeys(paneId, keys) {
    const r = await fetch('/api/pane/' + encodeURIComponent(paneId) + '/keys', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ keys }),
    });
    if (!r.ok) throw new Error('keys failed: ' + r.status);
    return r.json();
  },
  async setThinking(paneId, steps) {
    const r = await fetch('/api/pane/' + encodeURIComponent(paneId) + '/thinking', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ steps }),
    });
    if (!r.ok) throw new Error('thinking change failed: ' + r.status);
    return r.json();
  },
  async setModel(paneId, model) {
    const r = await fetch('/api/pane/' + encodeURIComponent(paneId) + '/model', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ model }),
    });
    const data = await r.json().catch(() => ({}));
    if (!r.ok) throw new Error(data.error || 'model change failed: ' + r.status);
    return data;
  },
  async createWorkspace(cwd) {
    const r = await fetch('/api/workspace', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ cwd }),
    });
    if (!r.ok) throw new Error('workspace create failed: ' + r.status);
    return r.json();
  },
  async closeWorkspace(workspaceId) {
    const r = await fetch('/api/workspace/' + encodeURIComponent(workspaceId) + '/close', { method: 'POST' });
    if (!r.ok) throw new Error('workspace close failed: ' + r.status);
    return r.json();
  },
  async createTab(workspaceId) {
    const r = await fetch('/api/tab', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ workspace_id: workspaceId }),
    });
    if (!r.ok) throw new Error('tab create failed: ' + r.status);
    return r.json();
  },
  async closeTab(tabId) {
    const r = await fetch('/api/tab/' + encodeURIComponent(tabId) + '/close', { method: 'POST' });
    if (!r.ok) throw new Error('tab close failed: ' + r.status);
    return r.json();
  },
};

// ------------------------------------------------------------
// Slash-command cache — fetched once, reused by the composer
// ------------------------------------------------------------
let commandsCache = null;
async function loadCommands() {
  if (commandsCache) return commandsCache;
  try {
    const data = await api.commands();
    commandsCache = (data && data.commands) || [];
  } catch (_) {
    commandsCache = [];
  }
  return commandsCache;
}

// Model catalog — served by the bridge from `omp models --json`, static per
// omp version. The in-flight promise is shared so session warm-up and a
// quickly opened picker never launch duplicate catalog processes.
let modelsCache = null;
let modelsPromise = null;
async function loadModels() {
  if (modelsCache) return modelsCache;
  if (modelsPromise) return modelsPromise;
  modelsPromise = (async () => {
    try {
      const r = await fetch('/api/models');
      if (!r.ok) return [];
      const d = await r.json();
      return (d && d.models) || [];
    } catch (_) {
      return [];
    }
  })();
  modelsCache = await modelsPromise;
  modelsPromise = null;
  return modelsCache;
}

export { api, loadCommands, loadModels };
