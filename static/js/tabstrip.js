import { api } from './api.js';
import { basename, clear, h } from './dom.js';
import { showToast } from './overlay.js';
import { isTabActive, navigateToTab, paneIndex, tabsIndex } from './state.js';

// ------------------------------------------------------------
// Shared tab strip — used by both the session and terminal views.
// Renders into #tabstrip-wrap / #tabstrip already present in the DOM.
// ------------------------------------------------------------
const TabStrip = {
  pendingClose: null,

  reset() {
    this.pendingClose = null;
  },

  render(paneId) {
    const stripWrap = document.getElementById('tabstrip-wrap');
    const strip = document.getElementById('tabstrip');
    if (!strip || !stripWrap) return;
    clear(strip);
    const pane = paneIndex.get(paneId);
    const wsId = pane ? pane.workspace_id : undefined;
    const tabs = wsId !== undefined ? (tabsIndex.get(wsId) || []) : [];
    if (tabs.length <= 1) {
      stripWrap.classList.add('empty');
      return;
    }
    stripWrap.classList.remove('empty');

    for (const tab of tabs) {
      const active = isTabActive(tab, pane);
      const firstPane = tab.pane_ids && tab.pane_ids[0] ? paneIndex.get(tab.pane_ids[0]) : null;
      const label = tab.label || (firstPane && (firstPane.title || basename(firstPane.cwd))) || tab.tab_id;
      const children = [h('span', { class: 'tab-chip-label' }, label)];
      if (active) {
        const confirming = this.pendingClose === tab.tab_id;
        children.push(h('span', {
          class: 'tab-chip-x' + (confirming ? ' confirm' : ''),
          onclick: (e) => { e.stopPropagation(); this.handleClose(tab, paneId); },
        }, confirming ? 'confirm?' : '\u00d7'));
      }
      strip.appendChild(h('button', {
        class: 'tab-chip' + (active ? ' active' : ''),
        onclick: () => { if (!active) navigateToTab(tab); },
      }, children));
    }

    strip.appendChild(h('button', {
      class: 'tab-chip tab-chip-add',
      'aria-label': 'New tab',
      onclick: () => this.handleNewTab(wsId, paneId),
    }, '+'));
  },

  handleClose(tab, paneId) {
    if (this.pendingClose === tab.tab_id) {
      this.pendingClose = null;
      api.closeTab(tab.tab_id).then(() => {
        location.hash = '#/';
      }).catch(() => showToast('Failed to close tab'));
      return;
    }
    this.pendingClose = tab.tab_id;
    this.render(paneId);
    setTimeout(() => {
      if (this.pendingClose === tab.tab_id) {
        this.pendingClose = null;
        this.render(paneId);
      }
    }, 3000);
  },

  async handleNewTab(wsId) {
    if (wsId === undefined) return;
    try {
      const res = await api.createTab(wsId);
      if (res && res.pane_id) location.hash = '#/term/' + encodeURIComponent(res.pane_id);
    } catch (e) {
      showToast('Failed to create tab');
    }
  },
};

export { TabStrip };
