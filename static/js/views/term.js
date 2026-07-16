import { api } from '../api.js';
import { app, basename, clear, h } from '../dom.js';
import { avatarEl, svgIcon } from '../icons.js';
import { showToast } from '../overlay.js';
import { sse } from '../sse.js';
import { paneIndex, refreshFleet, workspaceIndex } from '../state.js';
import { TabStrip } from '../tabstrip.js';

// ============================================================
// TERMINAL VIEW — raw screen for any pane (agent or shell)
// ============================================================
const TermView = {
  paneId: null,
  pollTimer: null,
  visHandler: null,

  mount(paneId) {
    this.paneId = paneId;
    TabStrip.reset();
    clear(app);

    const wrap = h('div', { class: 'view term-view' });
    const hdr = h('div', { class: 'hdr' }, [
      h('button', { class: 'back-btn', 'aria-label': 'Back', html: svgIcon('chevron-left', 22), onclick: () => { location.hash = '#/'; } }),
      h('div', { id: 'term-avatar-slot' }),
      h('div', { class: 'hdr-title-col' }, [
        h('h1', { id: 'term-title' }, '\u2026'),
        h('span', { class: 'sub', id: 'term-sub' }, ''),
      ]),
    ]);
    const tabStripWrap = h('div', { class: 'tabstrip-wrap', id: 'tabstrip-wrap' }, [
      h('div', { class: 'tabstrip', id: 'tabstrip' }),
    ]);
    const screenWrap = h('div', { class: 'scroll term-screen-wrap', id: 'term-screen-wrap' }, [
      h('pre', { class: 'term-screen', id: 'term-screen' }, ''),
    ]);
    const textInput = h('input', {
      type: 'text',
      id: 'term-text-input',
      placeholder: 'Type and send\u2026',
      autocapitalize: 'off',
      autocorrect: 'off',
      spellcheck: 'false',
      onkeydown: (e) => { if (e.key === 'Enter') { e.preventDefault(); this.handleSendText(); } },
      onfocus: () => this.handleFocus(),
    });
    const sendBtn = h('button', { class: 'term-send-btn', onclick: () => this.handleSendText() }, 'Send');
    const keyRow = h('div', { class: 'term-keys-row' }, [
      this.keyBtn('Enter', ['Enter']),
      this.keyBtn('Esc', ['Escape']),
      this.keyBtn('Ctrl+C', ['C-c']),
      this.keyBtn('Up', ['Up']),
      this.keyBtn('Down', ['Down']),
      this.keyBtn('Tab', ['Tab']),
    ]);
    const composer = h('div', { class: 'term-composer kb-pin', id: 'term-composer' }, [
      h('div', { class: 'term-composer-row' }, [textInput, sendBtn]),
      keyRow,
    ]);

    wrap.appendChild(hdr);
    wrap.appendChild(tabStripWrap);
    wrap.appendChild(screenWrap);
    wrap.appendChild(composer);
    app.appendChild(wrap);

    sse.onFleetEvent = () => { refreshFleet().then(() => this.updateHeader()).catch(() => {}); };
    sse.onSessionEvent = null;
    sse.onStateChange = () => {};

    refreshFleet().then(() => this.updateHeader()).catch(() => {});
    this.startPolling();

    this.visHandler = () => { if (document.hidden) this.stopPolling(); else this.startPolling(); };
    document.addEventListener('visibilitychange', this.visHandler);
  },

  unmount() {
    this.stopPolling();
    if (this.visHandler) { document.removeEventListener('visibilitychange', this.visHandler); this.visHandler = null; }
    sse.onFleetEvent = null;
  },

  onVisible() {
    refreshFleet().then(() => this.updateHeader()).catch(() => {});
    this.startPolling();
  },

  scrollToBottom(force) {
    const wrap = document.getElementById('term-screen-wrap');
    if (!wrap) return;
    if (force) wrap.scrollTop = wrap.scrollHeight;
  },

  keyBtn(label, keys) {
    return h('button', { class: 'term-key-btn', onclick: () => this.handleKeys(keys) }, label);
  },

  updateHeader() {
    if (this.paneGone()) return;
    const pane = paneIndex.get(this.paneId);
    const titleEl = document.getElementById('term-title');
    const subEl = document.getElementById('term-sub');
    if (titleEl) titleEl.textContent = (pane && (pane.title || basename(pane.cwd))) || basename(this.paneId) || this.paneId;
    const wsLabel = pane ? (workspaceIndex.get(pane.workspace_id) || pane.workspace_id || '') : '';
    if (subEl) subEl.textContent = wsLabel;
    const slot = document.getElementById('term-avatar-slot');
    if (slot) {
      const wsKey = pane ? (pane.workspace_id || wsLabel || this.paneId) : this.paneId;
      if (slot._wsKey !== wsKey) {
        clear(slot);
        if (wsLabel) slot.appendChild(avatarEl(wsLabel, 'sm'));
        slot._wsKey = wsKey;
      }
    }
    TabStrip.render(this.paneId);
  },

  // Same contract as SessionView.paneGone — herdr already closed the pane
  // (e.g. `exit` in a shell); leave the view instead of showing a corpse.
  paneGone() {
    if (paneIndex.size > 0 && !paneIndex.get(this.paneId)) {
      this.stopPolling();
      showToast('Pane closed');
      location.hash = '#/';
      return true;
    }
    return false;
  },

  startPolling() {
    this.stopPolling();
    this.loadScreen();
    this.pollTimer = setInterval(() => this.loadScreen(), 1000);
  },

  stopPolling() {
    if (this.pollTimer) { clearInterval(this.pollTimer); this.pollTimer = null; }
  },

  async loadScreen() {
    const pre = document.getElementById('term-screen');
    if (!pre) return;
    try {
      const data = await api.screen(this.paneId);
      const wrap = document.getElementById('term-screen-wrap');
      const wasNearBottom = wrap ? (wrap.scrollHeight - wrap.scrollTop - wrap.clientHeight) <= 40 : true;
      pre.textContent = (data && data.text) || '';
      if (wrap && wasNearBottom) wrap.scrollTop = wrap.scrollHeight;
    } catch (err) {
      if (err && err.status === 404) {
        // pane is gone at the source — don't wait for the next fleet poke
        this.stopPolling();
        showToast('Pane closed');
        location.hash = '#/';
        return;
      }
      // keep last known screen on transient failure; avoid spamming a toast every second
    }
  },

  handleFocus() {
    setTimeout(() => { this.scrollToBottom(true); }, 300);
  },

  async handleSendText() {
    const input = document.getElementById('term-text-input');
    if (!input) return;
    const text = input.value;
    if (!text) return;
    input.value = '';
    try {
      await api.sendText(this.paneId, text);
      this.loadScreen();
    } catch (err) {
      showToast('Failed to send');
    }
  },

  async handleKeys(keys) {
    try {
      await api.sendKeys(this.paneId, keys);
      this.loadScreen();
    } catch (err) {
      showToast('Failed to send keys');
    }
  },
};

export { TermView };
