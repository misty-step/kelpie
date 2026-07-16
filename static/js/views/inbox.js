import { api } from '../api.js';
import { app, basename, clear, h, prefersReducedMotion } from '../dom.js';
import { avatarEl, svgIcon } from '../icons.js';
import { sse } from '../sse.js';
import { paneStatus, refreshFleet, workspaceIndex } from '../state.js';

// ============================================================
// INBOX VIEW
// ============================================================
const InboxView = {
  data: null,
  cardNodes: new Map(), // pane_id -> DOM node, reused across renders (FLIP)
  sectionLabelNode: null,
  dialogBusy: false,

  mount() {
    clear(app);
    this.cardNodes = new Map();
    this.sectionLabelNode = null;
    const wrap = h('div', { class: 'view inbox-view', style: 'display:flex;flex-direction:column;height:100%;min-height:0;' });
    const hdr = h('div', { class: 'hdr' }, [
      h('div', { class: 'kelpie-brand' }, [
        h('span', { class: 'kelpie-mark' }, [h('img', { src: 'kelpie-mark.png', alt: '', width: '28', height: '28' })]),
        h('h1', null, 'kelpie'),
      ]),
      h('span', { class: 'dot ' + (sse.connected ? 'up' : 'down'), id: 'conn-dot' }),
      h('button', {
        class: 'hdr-icon-btn',
        id: 'new-ws-btn',
        'aria-label': 'New workspace',
        html: svgIcon('plus', 20),
        onclick: () => this.openDialog(),
      }),
    ]);
    const list = h('div', { class: 'scroll inbox-list', id: 'inbox-list' }, [
      h('div', { class: 'skeleton' }),
      h('div', { class: 'skeleton' }),
      h('div', { class: 'skeleton' }),
    ]);
    wrap.appendChild(hdr);
    wrap.appendChild(list);
    wrap.appendChild(this.buildDialog());
    app.appendChild(wrap);

    sse.onFleetEvent = () => this.load();
    sse.onStateChange = () => this.updateDot();
    this.updateDot();
    this.load();
  },

  unmount() {
    sse.onFleetEvent = null;
    sse.onStateChange = null;
  },

  onVisible() {
    this.load();
  },

  updateDot() {
    const dot = document.getElementById('conn-dot');
    if (dot) dot.className = 'dot ' + (sse.connected ? 'up' : 'down');
  },

  async load() {
    try {
      const data = await refreshFleet();
      this.data = data;
      this.render(data);
    } catch (err) {
      this.renderError();
    }
  },

  renderError() {
    const list = document.getElementById('inbox-list');
    if (!list) return;
    for (const node of this.cardNodes.values()) node.remove();
    this.cardNodes.clear();
    if (this.sectionLabelNode) { this.sectionLabelNode.remove(); this.sectionLabelNode = null; }
    clear(list);
    list.appendChild(h('div', { class: 'error-state' }, [
      h('div', null, "Couldn't load agents."),
      h('button', { class: 'retry-btn', onclick: () => this.load() }, 'Retry'),
    ]));
  },

  render(data) {
    const list = document.getElementById('inbox-list');
    if (!list) return;

    const panes = (data && data.panes) || [];
    const agentPanes = panes.filter((p) => p && p.agent);
    const shellPanes = panes.filter((p) => p && !p.agent);

    // Attention tier first (pending ask > working > idle/done), then a
    // STABLE alphabetical order within tier — recency reordering made the
    // inbox churn constantly and impossible to scan.
    const order = { blocked: 0, working: 1, idle: 2, done: 3, unknown: 4 };
    const wsName = (p) => (workspaceIndex.get(p.workspace_id) || p.workspace_id || p.title || p.cwd || p.pane_id || '').toLowerCase();
    const tier = (p) => p.pending_ask ? 0 : (order[paneStatus(p)] !== undefined ? order[paneStatus(p)] : 5);
    agentPanes.sort((a, b) => {
      const sa = tier(a), sb = tier(b);
      if (sa !== sb) return sa - sb;
      const na = wsName(a), nb = wsName(b);
      if (na !== nb) return na < nb ? -1 : 1;
      return a.pane_id < b.pane_id ? -1 : 1;
    });
    shellPanes.sort((a, b) => {
      const na = wsName(a), nb = wsName(b);
      if (na !== nb) return na < nb ? -1 : 1;
      return a.pane_id < b.pane_id ? -1 : 1;
    });

    this.flipRender(list, agentPanes, shellPanes);
  },

  // FLIP: (F)irst — snapshot old rects, (L)ast — reorder/update DOM,
  // (I)nvert — apply a transform from old->new delta, (P)lay — animate to zero.
  flipRender(list, agentPanes, shellPanes) {
    const reduced = prefersReducedMotion();

    const oldRects = new Map();
    for (const [key, node] of this.cardNodes) {
      if (node.isConnected) oldRects.set(key, node.getBoundingClientRect());
    }

    const staleState = list.querySelector('.empty-state, .error-state');
    if (staleState) staleState.remove();
    for (const skel of list.querySelectorAll('.skeleton')) skel.remove();

    if (agentPanes.length === 0 && shellPanes.length === 0) {
      for (const node of this.cardNodes.values()) node.remove();
      this.cardNodes.clear();
      if (this.sectionLabelNode) { this.sectionLabelNode.remove(); this.sectionLabelNode = null; }
      clear(list);
      list.appendChild(h('div', { class: 'empty-state' }, [
        h('span', { class: 'empty-icon', html: svgIcon('inbox', 48) }),
        h('div', null, 'No agents running.'),
        h('div', { class: 'empty-hint' }, 'Tap + to open a workspace.'),
      ]));
      return;
    }

    // Fade out and drop cards no longer present.
    const newKeys = new Set(agentPanes.map((p) => p.pane_id).concat(shellPanes.map((p) => p.pane_id)));
    for (const [key, node] of Array.from(this.cardNodes)) {
      if (newKeys.has(key)) continue;
      this.cardNodes.delete(key);
      if (!node.isConnected) continue;
      if (reduced) { node.remove(); continue; }
      const anim = node.animate(
        [{ opacity: 1, transform: 'translateY(0)' }, { opacity: 0, transform: 'translateY(-6px)' }],
        { duration: 180, easing: 'ease', fill: 'forwards' }
      );
      anim.onfinish = () => node.remove();
    }

    // Reuse or create nodes for every current pane, updating content in place.
    const newNodes = new Set();
    const ensureCard = (pane, isTerminal) => {
      let node = this.cardNodes.get(pane.pane_id);
      if (!node) {
        node = this.buildCard(pane, isTerminal);
        this.cardNodes.set(pane.pane_id, node);
        newNodes.add(node);
      } else {
        this.updateCard(node, pane, isTerminal);
      }
      return node;
    };

    const orderedNodes = agentPanes.map((p) => ensureCard(p, false));

    if (shellPanes.length > 0) {
      if (!this.sectionLabelNode) this.sectionLabelNode = h('div', { class: 'section-label' });
      this.sectionLabelNode.textContent = 'Terminals (' + shellPanes.length + ')';
      orderedNodes.push(this.sectionLabelNode);
      for (const p of shellPanes) orderedNodes.push(ensureCard(p, true));
    } else if (this.sectionLabelNode) {
      this.sectionLabelNode.remove();
      this.sectionLabelNode = null;
    }

    // Reorder: appendChild in desired order moves existing nodes and
    // appends new ones, producing the final order in a single pass.
    for (const node of orderedNodes) list.appendChild(node);

    if (reduced) return;

    for (const node of orderedNodes) {
      if (newNodes.has(node)) {
        node.animate(
          [{ opacity: 0, transform: 'translateY(10px)' }, { opacity: 1, transform: 'translateY(0)' }],
          { duration: 220, easing: 'ease-out', fill: 'both' }
        );
        continue;
      }
      const key = node.dataset ? node.dataset.paneId : undefined;
      if (!key) continue;
      const old = oldRects.get(key);
      if (!old) continue;
      const rect = node.getBoundingClientRect();
      const dy = old.top - rect.top;
      if (Math.abs(dy) < 1) continue;
      node.animate(
        [{ transform: 'translateY(' + dy + 'px)' }, { transform: 'translateY(0)' }],
        { duration: 250, easing: 'cubic-bezier(0.22,1,0.36,1)' }
      );
    }
  },

  buildCard(pane, isTerminal) {
    const titleEl = h('div', { class: 'card-title' });
    const chipEl = h('span', { class: 'chip' });
    const paneId = pane.pane_id;
    const dest = isTerminal ? '#/term/' + encodeURIComponent(paneId) : '#/pane/' + encodeURIComponent(paneId);
    const btn = h('button', {
      class: 'card',
      onclick: () => { location.hash = dest; },
    }, [titleEl, chipEl]);
    btn.dataset.paneId = paneId;
    btn._refs = { titleEl, chipEl };
    this.updateCard(btn, pane, isTerminal);
    return btn;
  },

  // Card = avatar + workspace name + status chip. One row, nothing else —
  // the inbox is a scannable roster, not a feed.
  updateCard(node, pane, isTerminal) {
    const refs = node._refs;
    const status = paneStatus(pane);
    const wsLabel = workspaceIndex.get(pane.workspace_id) || pane.workspace_id || '';
    const title = pane.title || basename(pane.cwd) || pane.pane_id || 'pane';

    // Avatar: deterministic per workspace name. Rebuild only if the workspace
    // changed (new pane reusing a recycled card slot) to avoid per-paint churn.
    const wsKey = pane.workspace_id || wsLabel || pane.pane_id || '';
    if (node._wsKey !== wsKey) {
      const old = node.querySelector('.avatar');
      if (old) old.remove();
      const av = avatarEl(wsLabel || wsKey);
      node.insertBefore(av, refs.titleEl);
      node._wsKey = wsKey;
    }

    refs.titleEl.textContent = wsLabel || title;

    if (isTerminal) {
      refs.chipEl.style.display = 'none';
    } else {
      refs.chipEl.style.display = '';
      refs.chipEl.className = 'chip ' + (pane.pending_ask ? 'blocked' : status);
      refs.chipEl.textContent = pane.pending_ask ? 'needs input' : status;
    }
  },

  // ---- new-workspace dialog (item 8a) ----
  buildDialog() {
    const overlay = h('div', { class: 'dialog-overlay hidden', id: 'ws-dialog-overlay' });
    const cwdInput = h('input', {
      type: 'text',
      id: 'ws-cwd-input',
      placeholder: '~/Development/...',
      autocapitalize: 'off',
      autocorrect: 'off',
      spellcheck: 'false',
      onkeydown: (e) => { if (e.key === 'Enter') { e.preventDefault(); this.submitDialog(); } },
    });
    const errEl = h('div', { class: 'dialog-error', id: 'ws-dialog-error', style: 'display:none;' });
    const createBtn = h('button', { class: 'dialog-create-btn', id: 'ws-create-btn', onclick: () => this.submitDialog() }, 'Create');
    const panel = h('div', { class: 'dialog-panel' }, [
      h('h2', null, 'New workspace'),
      cwdInput,
      errEl,
      h('div', { class: 'dialog-actions' }, [
        h('button', { class: 'dialog-cancel-btn', onclick: () => this.closeDialog() }, 'Cancel'),
        createBtn,
      ]),
    ]);
    overlay.appendChild(panel);
    overlay.addEventListener('click', (e) => { if (e.target === overlay) this.closeDialog(); });
    return overlay;
  },

  openDialog() {
    const overlay = document.getElementById('ws-dialog-overlay');
    if (!overlay) return;
    overlay.classList.remove('hidden');
    const input = document.getElementById('ws-cwd-input');
    const err = document.getElementById('ws-dialog-error');
    if (input) { input.value = ''; setTimeout(() => input.focus(), 50); }
    if (err) err.style.display = 'none';
  },

  closeDialog() {
    const overlay = document.getElementById('ws-dialog-overlay');
    if (overlay) overlay.classList.add('hidden');
  },

  async submitDialog() {
    if (this.dialogBusy) return;
    const input = document.getElementById('ws-cwd-input');
    const err = document.getElementById('ws-dialog-error');
    const btn = document.getElementById('ws-create-btn');
    const cwd = input ? input.value.trim() : '';
    if (!cwd) {
      if (err) { err.textContent = 'Enter a directory path.'; err.style.display = ''; }
      return;
    }
    this.dialogBusy = true;
    if (btn) btn.disabled = true;
    try {
      const res = await api.createWorkspace(cwd);
      this.closeDialog();
      if (res && res.pane_id) location.hash = '#/term/' + encodeURIComponent(res.pane_id);
      else this.load();
    } catch (e) {
      if (err) { err.textContent = 'Failed to create workspace.'; err.style.display = ''; }
    } finally {
      this.dialogBusy = false;
      if (btn) btn.disabled = false;
    }
  },
};

export { InboxView };
