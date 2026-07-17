import { api, loadCommands, loadModels } from '../api.js';
import { app, basename, clear, h, relTime } from '../dom.js';
import { avatarEl, svgIcon } from '../icons.js';
import { renderMarkdown } from '../markdown.js';
import { openSheet, showToast } from '../overlay.js';
import { sse } from '../sse.js';
import { paneIndex, paneStatus, refreshFleet, workspaceIndex } from '../state.js';
import { TabStrip } from '../tabstrip.js';

// ============================================================
// SESSION VIEW
// ============================================================
const SessionView = {
  paneId: null,
  data: null,
  answering: false,
  attachments: [],
  uploading: 0,
  thinkingExpanded: new Set(),
  toolExpanded: new Set(),
  thinkingOverride: null,
  thinkingApplying: false,
  modelOverride: null,
  modelApplying: false,

  mount(paneId) {
    this.paneId = paneId;
    this.data = null;
    this.answering = false;
    this.attachments = [];
    this.uploading = 0;
    this.thinkingExpanded = new Set();
    this.toolExpanded = new Set();
    this.thinkingOverride = null;
    this.thinkingApplying = false;
    this.modelOverride = null;
    this.modelApplying = false;
    loadModels();
    TabStrip.reset();
    clear(app);

    const wrap = h('div', { class: 'view session-view' });
    const hdr = h('div', { class: 'hdr' }, [
      h('button', { class: 'back-btn', 'aria-label': 'Back', html: svgIcon('chevron-left', 22), onclick: () => { location.hash = '#/'; } }),
      h('div', { id: 'session-avatar-slot' }),
      h('div', { class: 'hdr-title-col' }, [
        h('h1', { id: 'session-title' }, '\u2026'),
        h('span', { class: 'sub', id: 'session-sub' }, ''),
      ]),
      h('span', { class: 'chip unknown', id: 'session-chip' }, ''),
      h('button', {
        class: 'hdr-icon-btn',
        id: 'term-link-btn',
        'aria-label': 'Open terminal',
        html: svgIcon('terminal', 18),
        onclick: () => { location.hash = '#/term/' + encodeURIComponent(this.paneId); },
      }),
    ]);

    const tabStripWrap = h('div', { class: 'tabstrip-wrap', id: 'tabstrip-wrap' }, [
      h('div', { class: 'tabstrip', id: 'tabstrip' }),
    ]);

    const scrollWrap = h('div', { class: 'session-scroll-wrap' });
    const scroll = h('div', { class: 'scroll transcript', id: 'transcript' });
    scrollWrap.appendChild(scroll);
    const jumpPill = h('button', { class: 'jump-pill', id: 'jump-pill', style: 'display:none;', onclick: () => this.scrollToBottom(true) }, 'Jump to latest');
    scrollWrap.appendChild(jumpPill);

    const askBox = h('div', { class: 'ask-box', id: 'ask-box', style: 'display:none;' });

    const cmdSuggest = h('div', { class: 'cmd-suggest', id: 'cmd-suggest', style: 'display:none;' });
    const attachRow = h('div', { class: 'attach-row', id: 'attach-row', style: 'display:none;' });
    const statusRow = h('div', { class: 'composer-status-row' }, [
      h('button', { class: 'ctx-chip', id: 'model-chip-btn', 'aria-label': 'Switch model', onclick: () => this.openModelSheet() },
        [h('span', { class: 'ctx-chip-pill', id: 'model-chip-label' }, '\u2026')]),
      h('button', { class: 'ctx-chip', id: 'thinking-chip-btn', 'aria-label': 'Choose reasoning effort', style: 'display:none;', onclick: () => this.openThinkingSheet() },
        [h('span', { class: 'ctx-chip-pill', id: 'thinking-chip-label' }, '')]),
      h('div', { class: 'ctx-spacer' }),
      h('button', { class: 'more-btn', id: 'more-btn', 'aria-label': 'More actions', html: svgIcon('ellipsis', 18), onclick: () => this.openActionSheet() }),
      h('button', { class: 'status-esc-btn', id: 'esc-btn', onclick: () => this.handleInterrupt() }, 'Esc'),
      h('button', { class: 'status-send-btn', id: 'send-btn', 'aria-label': 'Send', disabled: true, html: svgIcon('send', 18), onclick: () => this.handleSend() }),
    ]);
    const fileInput = h('input', {
      type: 'file',
      id: 'attach-input',
      accept: 'image/*',
      multiple: 'multiple',
      style: 'display:none;',
      onchange: (e) => this.handleAttachFiles(e),
    });
    const textareaRow = h('div', { class: 'composer-textarea-row' }, [
      h('button', {
        class: 'attach-btn',
        id: 'attach-btn',
        'aria-label': 'Attach photo',
        html: svgIcon('plus', 20),
        onclick: () => { const inp = document.getElementById('attach-input'); if (inp) inp.click(); },
      }),
      h('textarea', {
        id: 'composer-input',
        rows: '1',
        placeholder: 'Message the agent\u2026',
        oninput: (e) => this.handleInput(e),
        onkeydown: (e) => this.handleKeydown(e),
        onfocus: (e) => this.handleFocus(e),
      }),
      fileInput,
    ]);
    const composerWrap = h('div', { class: 'composer-wrap kb-pin', id: 'composer-wrap' }, [cmdSuggest, attachRow, statusRow, textareaRow]);

    wrap.appendChild(hdr);
    wrap.appendChild(tabStripWrap);
    wrap.appendChild(scrollWrap);
    wrap.appendChild(askBox);
    wrap.appendChild(composerWrap);
    app.appendChild(wrap);

    scroll.addEventListener('scroll', () => this.handleScroll());

    sse.onSessionEvent = (pid) => { if (pid === this.paneId) this.load(); };
    sse.onFleetEvent = () => {
      refreshFleet().then(() => {
        if (this.paneGone()) return;
        this.updateChip();
        TabStrip.render(this.paneId);
      }).catch(() => {});
    };
    sse.onStateChange = () => {};

    this.updateChip();
    refreshFleet().then(() => { this.updateChip(); TabStrip.render(this.paneId); }).catch(() => {});
    this.load();
  },

  unmount() {
    sse.onSessionEvent = null;
    sse.onFleetEvent = null;
  },

  // herdr closes the pane (and its tab) itself when the process exits —
  // e.g. typing `exit` in a shell. Detect via fleet refresh and bail out.
  paneGone() {
    if (paneIndex.size > 0 && !paneIndex.get(this.paneId)) {
      showToast('Pane closed');
      location.hash = '#/';
      return true;
    }
    return false;
  },

  onVisible() {
    this.load();
    refreshFleet().then(() => { this.updateChip(); TabStrip.render(this.paneId); }).catch(() => {});
  },

  updateChip() {
    const chipEl = document.getElementById('session-chip');
    const subEl = document.getElementById('session-sub');
    const p = paneIndex.get(this.paneId);
    const status = paneStatus(p);
    if (chipEl) { chipEl.className = 'chip ' + status; chipEl.textContent = status; }
    if (subEl) {
      const wsLabel = p ? (workspaceIndex.get(p.workspace_id) || p.workspace_id || '') : '';
      subEl.textContent = wsLabel;
      const slot = document.getElementById('session-avatar-slot');
      if (slot) {
        const wsKey = p ? (p.workspace_id || wsLabel || this.paneId) : this.paneId;
        if (slot._wsKey !== wsKey) {
          clear(slot);
          if (wsLabel) slot.appendChild(avatarEl(wsLabel, 'sm'));
          slot._wsKey = wsKey;
        }
      }
    }
  },

  isNearBottom() {
    const scroll = document.getElementById('transcript');
    if (!scroll) return true;
    return (scroll.scrollHeight - scroll.scrollTop - scroll.clientHeight) <= 80;
  },

  scrollToBottom(force) {
    const scroll = document.getElementById('transcript');
    if (!scroll) return;
    if (force || this.isNearBottom()) {
      scroll.scrollTop = scroll.scrollHeight;
    }
    const pill = document.getElementById('jump-pill');
    if (pill) pill.style.display = 'none';
  },

  handleScroll() {
    const pill = document.getElementById('jump-pill');
    if (!pill) return;
    pill.style.display = this.isNearBottom() ? 'none' : 'block';
  },

  async load() {
    const wasNearBottom = this.isNearBottom();
    try {
      const data = await api.session(this.paneId);
      this.data = data;
      this.render(data, wasNearBottom);
    } catch (err) {
      this.renderError();
    }
  },

  renderError() {
    const scroll = document.getElementById('transcript');
    if (!scroll) return;
    if (scroll.children.length > 0) {
      showToast("Couldn't refresh session");
      return;
    }
    clear(scroll);
    scroll.appendChild(h('div', { class: 'error-state' }, [
      h('div', null, "Couldn't load session."),
      h('button', { class: 'retry-btn', onclick: () => this.load() }, 'Retry'),
    ]));
  },

  render(data, wasNearBottom) {
    const titleEl = document.getElementById('session-title');
    if (titleEl) {
      const cached = paneIndex.get(this.paneId);
      const fallback = (cached && (cached.title || basename(cached.cwd))) || basename(this.paneId) || this.paneId;
      titleEl.textContent = (data && data.title) || fallback;
    }

    const scroll = document.getElementById('transcript');
    if (scroll) {
      clear(scroll);
      const entries = (data && data.entries) || [];
      if (entries.length === 0) {
        scroll.appendChild(h('div', { class: 'empty-state' }, 'No messages yet.'));
      } else {
        for (let i = 0; i < entries.length; i++) {
          const node = this.renderEntry(entries[i], i);
          if (node) scroll.appendChild(node);
        }
      }
      requestAnimationFrame(() => this.scrollToBottom(wasNearBottom));
    }

    this.updateComposerStatus(data);
    this.renderAsk(data && data.pending_ask);
  },

  updateComposerStatus(data) {
    const mLabel = document.getElementById('model-chip-label');
    const tBtn = document.getElementById('thinking-chip-btn');
    const tLabel = document.getElementById('thinking-chip-label');
    if (!mLabel) return;
    const model = data && data.model;
    if (this.modelOverride && model
      && (model.provider || '') + '/' + (model.model || '') === this.modelOverride.selector) {
      this.modelOverride = null;
    }
    if (this.modelOverride) {
      mLabel.textContent = this.modelOverride.label;
    } else {
      let name = (model && (model.model || model.provider)) || '';
      if (name.includes('/')) name = name.split('/').pop();
      const provider = (model && model.provider) || '';
      mLabel.textContent = provider && name !== provider
        ? provider + ' · ' + name
        : (name || 'model \u2026');
    }
    const reportedThinking = (data && data.thinking) || '';
    if (this.thinkingOverride && reportedThinking === this.thinkingOverride.value) {
      this.thinkingOverride = null;
    }
    const thinking = this.thinkingOverride ? this.thinkingOverride.value : reportedThinking;
    if (tBtn && tLabel) {
      if (thinking) {
        tLabel.textContent = this.thinkingLabel(this.normalizeThinking(thinking));
        tBtn.style.display = '';
      } else {
        tBtn.style.display = 'none';
      }
    }
  },

  // Tap the model chip -> full catalog picker. Selection drives omp's own
  // interactive `/model` picker through the bridge (omp does not execute
  // arged slash commands submitted as composer text); the bridge verifies
  // each picker stage and the printed `Default model:` receipt before
  // reporting success, so the chip only flips on a confirmed switch.
  openModelSheet() {
    // A confirmed-but-unreconciled switch outranks the (lagging) session file.
    const current = this.modelOverride
      ? {
          provider: this.modelOverride.selector.split('/')[0],
          model: this.modelOverride.selector.split('/').slice(1).join('/'),
        }
      : (this.data && this.data.model) || null;
    const view = this;
    const MAX_ROWS = 60;
    openSheet((sheet, close) => {
      sheet.appendChild(h('div', { class: 'sheet-title' }, 'Model'));
      const search = h('input', {
        class: 'sheet-search',
        type: 'search',
        placeholder: 'Filter models\u2026',
        autocapitalize: 'off',
        autocorrect: 'off',
        spellcheck: 'false',
      });
      sheet.appendChild(h('div', { class: 'sheet-search-wrap' }, [search]));
      const list = h('div', { class: 'sheet-scroll' }, [h('div', { class: 'sheet-hint' }, 'Loading\u2026')]);
      sheet.appendChild(list);

      const rowFor = (m) => {
        const isCur = current && m.id === current.model;
        return h('button', {
          class: 'sheet-row' + (isCur ? ' current' : ''),
          onclick: async () => {
            close();
            if (isCur || view.modelApplying) return;
            const btn = document.getElementById('model-chip-btn');
            const label = document.getElementById('model-chip-label');
            let shortName = m.name || m.id;
            const chipText = m.provider + ' \u00b7 ' + shortName;
            view.modelApplying = true;
            if (btn) {
              btn.disabled = true;
              btn.setAttribute('aria-busy', 'true');
            }
            if (label) label.textContent = 'switching\u2026';
            view.updateSendState();
            try {
              // Capture the live effort first: omp resets a freshly
              // assigned model role to 'auto'.
              const prevLevel = view.normalizeThinking(
                await view.readLiveThinking().catch(() => '')
                || (view.thinkingOverride && view.thinkingOverride.value)
                || (view.data && view.data.thinking));
              await api.setModel(view.paneId, m.selector);
              view.modelOverride = { selector: m.selector, label: chipText };
              if (label) label.textContent = chipText;
              showToast('Model: ' + shortName, 'info');
              await view.restoreThinkingAfterSwitch(m, prevLevel);
            } catch (err) {
              view.updateComposerStatus(view.data);
              showToast(err && err.message ? err.message : 'Failed to switch model');
            } finally {
              view.modelApplying = false;
              if (btn) {
                btn.disabled = false;
                btn.removeAttribute('aria-busy');
              }
              view.updateSendState();
              view.load();
            }
          },
        }, [
          h('span', { class: 'sheet-row-label' }, m.name || m.id),
          h('span', { class: 'sheet-row-sub' }, m.provider + ' · ' + m.id + (Array.isArray(m.thinking) && m.thinking.length ? ' · ' + m.thinking.join('/') : '')),
        ]);
      };

      const renderList = (models, filter) => {
        clear(list);
        if (!models.length) {
          list.appendChild(h('div', { class: 'sheet-hint' }, 'Model list unavailable.'));
          return;
        }
        const q = (filter || '').trim().toLowerCase();
        const matches = q
          ? models.filter((m) => (m.id + ' ' + (m.name || '') + ' ' + m.provider).toLowerCase().includes(q))
          : models;
        // current provider's group first, then alphabetical
        const groups = new Map();
        for (const m of matches) {
          const g = groups.get(m.provider) || [];
          g.push(m);
          groups.set(m.provider, g);
        }
        const provs = [...groups.keys()].sort((a, b) => {
          const ca = current && a === current.provider ? -1 : 0;
          const cb = current && b === current.provider ? -1 : 0;
          return ca - cb || a.localeCompare(b);
        });
        let shown = 0;
        for (const prov of provs) {
          if (shown >= MAX_ROWS) break;
          list.appendChild(h('div', { class: 'sheet-group' }, prov));
          for (const m of groups.get(prov)) {
            if (shown >= MAX_ROWS) break;
            list.appendChild(rowFor(m));
            shown++;
          }
        }
        if (matches.length > shown) {
          list.appendChild(h('div', { class: 'sheet-hint' },
            (matches.length - shown) + ' more \u2014 type to narrow'));
        }
        if (!matches.length) {
          list.appendChild(h('div', { class: 'sheet-hint' }, 'No models match.'));
        }
      };

      loadModels().then((models) => {
        renderList(models, '');
        search.addEventListener('input', () => renderList(models, search.value));
      });
    });
  },

  normalizeThinking(value) {
    const v = String(value || '').trim().toLowerCase();
    if (v.startsWith('min')) return 'minimal';
    if (v.startsWith('med')) return 'medium';
    if (v.startsWith('xhi')) return 'xhigh';
    return v;
  },

  thinkingLabel(level) {
    return ({
      off: 'Off',
      auto: 'Auto',
      minimal: 'Minimal',
      low: 'Low',
      medium: 'Medium',
      high: 'High',
      xhigh: 'Extra high',
      max: 'Max',
      unknown: 'Unknown',
    })[level] || level;
  },

  async readLiveThinking() {
    const scr = await api.screen(this.paneId);
    const re = /\u00b7\s*\S*\s*(off|min\w*|low|med\w*|high|xhi\w*|max|auto)\b/gi;
    let m, last = null;
    while ((m = re.exec((scr && scr.text) || '')) !== null) last = m[1];
    return this.normalizeThinking(last);
  },

  openThinkingSheet() {
    if (this.thinkingApplying) return;
    const view = this;
    const model = (this.data && this.data.model) || {};
    openSheet((sheet, close) => {
      sheet.appendChild(h('div', { class: 'sheet-title' }, 'Reasoning effort'));
      const list = h('div', { class: 'sheet-scroll' }, [
        h('div', { class: 'sheet-hint' }, 'Loading options\u2026'),
      ]);
      sheet.appendChild(list);

      Promise.all([
        loadModels(),
        view.readLiveThinking().catch(() => ''),
      ]).then(([models, live]) => {
        clear(list);
        const found = models.find((m) =>
          m.provider === model.provider && m.id === model.model);
        const available = found && Array.isArray(found.thinking) ? found.thinking : [];
        if (!available.length) {
          list.appendChild(h('div', { class: 'sheet-hint' },
            'This model does not expose reasoning effort controls.'));
          return;
        }
        const levels = [...new Set(['off', 'auto', ...available.map((v) => view.normalizeThinking(v))])];
        const reported = view.normalizeThinking(
          (view.thinkingOverride && view.thinkingOverride.value) ||
          live ||
          (view.data && view.data.thinking));
        const current = levels.includes(live) ? live : reported;
        if (live && view.thinkingOverride && view.thinkingOverride.value === 'unknown') {
          view.thinkingOverride = { base: view.thinkingOverride.base, value: live };
          const chipLabel = document.getElementById('thinking-chip-label');
          if (chipLabel) chipLabel.textContent = view.thinkingLabel(live);
        }
        list.appendChild(h('div', { class: 'sheet-context' },
          (found.name || found.id) + ' \u00b7 ' + found.provider));

        const descriptions = {
          off: 'No model reasoning',
          auto: 'Let the model choose',
          minimal: 'Lightest reasoning',
          low: 'Fastest',
          medium: 'Balanced',
          high: 'Deeper analysis',
          xhigh: 'Extra depth',
          max: 'Maximum effort',
        };
        for (const level of levels) {
          const isCurrent = level === current;
          list.appendChild(h('button', {
            class: 'sheet-row thinking-level-row' + (isCurrent ? ' current' : ''),
            'aria-pressed': isCurrent ? 'true' : 'false',
            onclick: () => {
              close();
              if (!isCurrent) view.applyThinking(level, current, levels);
            },
          }, [
            h('span', { class: 'sheet-row-copy' }, [
              h('span', { class: 'sheet-row-label' }, view.thinkingLabel(level)),
              h('span', { class: 'sheet-row-sub' }, descriptions[level] || ''),
            ]),
            isCurrent
              ? h('span', { class: 'thinking-level-check', html: svgIcon('check', 18) })
              : null,
          ]));
        }
      });
    });
  },

  async applyThinking(target, current, levels) {
    const from = levels.indexOf(current);
    const to = levels.indexOf(target);
    if (from < 0 || to < 0) {
      showToast('Could not read current reasoning effort');
      return;
    }
    const steps = (to - from + levels.length) % levels.length;
    if (!steps) return;

    const btn = document.getElementById('thinking-chip-btn');
    const label = document.getElementById('thinking-chip-label');
    this.thinkingApplying = true;
    this.thinkingOverride = { base: current, value: target };
    if (btn) {
      btn.disabled = true;
      btn.setAttribute('aria-busy', 'true');
    }
    if (label) label.textContent = this.thinkingLabel(target) + ' \u2026';
    this.updateSendState();

    try {
      await api.setThinking(this.paneId, steps);
      const live = await this.readLiveThinking();
      if (live !== target) throw new Error('thinking level mismatch');
      if (label) label.textContent = this.thinkingLabel(target);
      showToast('Reasoning: ' + this.thinkingLabel(target), 'info');
    } catch (_) {
      const live = await this.readLiveThinking().catch(() => null);
      const verified = live || 'unknown';
      this.thinkingOverride = { base: current, value: verified };
      if (label) label.textContent = this.thinkingLabel(verified);
      showToast(live
        ? 'Reasoning effort is ' + this.thinkingLabel(live)
        : 'Could not verify reasoning effort');
    } finally {
      this.thinkingApplying = false;
      if (btn) {
        btn.disabled = false;
        btn.removeAttribute('aria-busy');
      }
      this.updateSendState();
    }
  },

  // A fresh role assignment resets omp's effort to 'auto' (verified live);
  // put the pane's prior level back through the verified reasoning flow.
  // Best-effort: the model switch already succeeded, so failures here only
  // surface through applyThinking's own toasts, never as a switch failure.
  async restoreThinkingAfterSwitch(m, prevLevel) {
    try {
      if (!prevLevel || prevLevel === 'unknown' || prevLevel === 'auto') return;
      const available = Array.isArray(m.thinking)
        ? m.thinking.map((v) => this.normalizeThinking(v))
        : [];
      if (!available.length) return;
      const levels = [...new Set(['off', 'auto', ...available])];
      if (!levels.includes(prevLevel)) return;
      const live = await this.readLiveThinking().catch(() => '');
      if (!live || !levels.includes(live) || live === prevLevel) return;
      await this.applyThinking(prevLevel, live, levels);
    } catch (_) { /* best-effort */ }
  },

  openActionSheet() {
    const view = this;
    openSheet((sheet, close) => {
      sheet.appendChild(h('div', { class: 'sheet-title' }, 'Actions'));
      const p = paneIndex.get(view.paneId);
      const actions = [
        ['terminal', 'Open terminal', () => { location.hash = '#/term/' + encodeURIComponent(view.paneId); }],
        ['plus', 'New tab', () => TabStrip.handleNewTab(p && p.workspace_id, view.paneId)],
        ['arrow-down', 'Jump to latest', () => view.scrollToBottom(true)],
        ['corner-down-left', 'Send Enter', () => api.sendKeys(view.paneId, ['Enter']).catch(() => showToast('Failed to send key'))],
        ['square', 'Send Ctrl+C', () => api.sendKeys(view.paneId, ['ctrl+c']).catch(() => showToast('Failed to send key'))],
        ['inbox', 'Back to inbox', () => { location.hash = '#/'; }],
      ];
      for (const [icon, label, fn] of actions) {
        sheet.appendChild(h('button', { class: 'sheet-row sheet-action-row', onclick: () => { close(); fn(); } }, [
          h('span', { class: 'sheet-row-icon', html: svgIcon(icon, 18) }),
          h('span', { class: 'sheet-row-label' }, label),
        ]));
      }
    });
  },

  renderEntry(entry, idx) {
    if (!entry || !entry.kind) return null;
    const ts = entry.ts ? h('div', { class: 'entry-ts' }, relTime(entry.ts)) : null;

    if (entry.kind === 'user') {
      return h('div', { class: 'entry entry-user' }, [
        h('div', { class: 'bubble' }, entry.text || ''),
        ts,
      ]);
    }

    if (entry.kind === 'assistant') {
      const bubble = h('div', { class: 'bubble', html: renderMarkdown(entry.text || '') });
      return h('div', { class: 'entry entry-assistant' }, [bubble, ts]);
    }

    if (entry.kind === 'thinking') {
      const text = entry.text || '';
      const long = text.length > 240;
      const expanded = this.thinkingExpanded.has(idx);
      const collapsed = long && !expanded;
      const wrapEl = h('div', { class: 'entry entry-thinking' + (collapsed ? ' collapsed' : '') });
      wrapEl.appendChild(h('div', { class: 'bubble', html: renderMarkdown(text) }));
      if (long) {
        wrapEl.appendChild(h('button', {
          class: 'expand-toggle',
          onclick: () => {
            if (this.thinkingExpanded.has(idx)) this.thinkingExpanded.delete(idx);
            else this.thinkingExpanded.add(idx);
            this.render(this.data, false);
          },
        }, expanded ? 'Show less' : 'Show more'));
      }
      if (ts) wrapEl.appendChild(ts);
      return wrapEl;
    }

    if (entry.kind === 'tool') {
      const status = entry.status || 'pending';
      const isOpen = this.toolExpanded.has(idx);
      const card = h('div', { class: 'entry entry-tool tool-card' });
      const head = h('div', {
        class: 'tool-head',
        onclick: () => {
          if (this.toolExpanded.has(idx)) this.toolExpanded.delete(idx);
          else this.toolExpanded.add(idx);
          this.render(this.data, false);
        },
      }, [
        h('span', { class: 'tool-status ' + status }),
        h('span', { class: 'tool-name' }, entry.name || 'tool'),
        h('span', { class: 'tool-intent' }, entry.intent || ''),
      ]);
      card.appendChild(head);
      if (entry.result) {
        card.appendChild(h('div', { class: 'tool-result' + (isOpen ? '' : ' hidden') }, entry.result));
      }
      if (ts) card.appendChild(ts);
      return card;
    }

    return null;
  },

  renderAsk(ask) {
    const box = document.getElementById('ask-box');
    if (!box) return;
    clear(box);
    if (!ask || this.answering) {
      box.style.display = 'none';
      return;
    }
    box.style.display = 'flex';
    box.appendChild(h('div', { class: 'ask-question' }, ask.question || ''));
    const optWrap = h('div', { class: 'ask-options' });
    const options = ask.options || [];
    for (let i = 0; i < options.length; i++) {
      const opt = options[i];
      const isRec = ask.recommended === i;
      const btn = h('button', {
        class: 'ask-option' + (isRec ? ' recommended' : ''),
        onclick: () => this.handleAsk(i),
      }, [
        document.createTextNode(opt.label || ('Option ' + (i + 1))),
        opt.description ? h('span', { class: 'opt-desc' }, opt.description) : null,
      ]);
      optWrap.appendChild(btn);
    }
    box.appendChild(optWrap);
  },

  async handleAsk(index) {
    this.answering = true;
    this.renderAsk(null); // optimistically hide
    try {
      await api.sendAsk(this.paneId, index);
    } catch (err) {
      showToast('Failed to send answer');
      this.answering = false;
      this.load();
      return;
    }
    this.answering = false;
    this.load();
  },

  handleInput(e) {
    const ta = e.target;
    ta.style.height = 'auto';
    const maxH = 6 * 24; // ~6 lines
    ta.style.height = Math.min(ta.scrollHeight, maxH) + 'px';
    this.updateSendState();

    const val = ta.value;
    if (val.startsWith('/') && !/\s/.test(val)) {
      this.showSuggestions(val);
    } else {
      this.hideSuggestions();
    }
  },

  updateSendState() {
    const ta = document.getElementById('composer-input');
    const sendBtn = document.getElementById('send-btn');
    if (!sendBtn) return;
    const hasText = !!(ta && ta.value.trim().length > 0);
    const hasAttachments = this.attachments.length > 0;
    sendBtn.disabled = (!hasText && !hasAttachments) || this.uploading > 0 || this.thinkingApplying || this.modelApplying;
  },

  async handleAttachFiles(e) {
    const input = e.target;
    const files = Array.from(input.files || []);
    input.value = '';
    if (files.length === 0) return;
    for (const file of files) {
      const entry = { name: file.name || 'photo', path: null, pending: true };
      this.attachments.push(entry);
      this.renderAttachments();
      this.uploading++;
      this.updateSendState();
      try {
        const res = await api.upload(this.paneId, file);
        entry.path = res && res.path;
        entry.pending = false;
        if (!entry.path) throw new Error('no path');
      } catch (err) {
        this.attachments = this.attachments.filter((a) => a !== entry);
        showToast('Photo upload failed');
      } finally {
        this.uploading--;
        this.renderAttachments();
        this.updateSendState();
      }
    }
  },

  renderAttachments() {
    const row = document.getElementById('attach-row');
    if (!row) return;
    clear(row);
    if (this.attachments.length === 0) {
      row.style.display = 'none';
      return;
    }
    row.style.display = 'flex';
    for (const att of this.attachments) {
      const chipChildren = [
        h('span', { class: 'attach-chip-icon', html: svgIcon('image', 14) }),
        h('span', { class: 'attach-chip-name' }, att.pending ? 'Uploading\u2026' : att.name),
      ];
      if (!att.pending) {
        chipChildren.push(h('button', {
          class: 'attach-chip-x',
          'aria-label': 'Remove attachment',
          onclick: () => {
            this.attachments = this.attachments.filter((a) => a !== att);
            this.renderAttachments();
            this.updateSendState();
          },
        }, '\u00d7'));
      }
      row.appendChild(h('span', { class: 'attach-chip' + (att.pending ? ' pending' : '') }, chipChildren));
    }
  },

  async showSuggestions(val) {
    const panel = document.getElementById('cmd-suggest');
    if (!panel) return;
    const cmds = await loadCommands();
    const prefix = val.slice(1).toLowerCase();
    const matches = cmds.filter((c) => {
      if (!c || !c.name) return false;
      if (String(c.name).toLowerCase().startsWith(prefix)) return true;
      return (c.aliases || []).some((a) => String(a).toLowerCase().startsWith(prefix));
    }).slice(0, 6);

    // Composer value may have changed while we awaited the fetch.
    const ta = document.getElementById('composer-input');
    if (!ta || !ta.value.startsWith('/') || /\s/.test(ta.value)) { this.hideSuggestions(); return; }
    if (matches.length === 0) { this.hideSuggestions(); return; }

    clear(panel);
    for (const c of matches) {
      panel.appendChild(h('button', {
        class: 'cmd-row',
        onclick: () => this.applyCommand(c),
      }, [
        h('span', { class: 'cmd-name' }, '/' + c.name),
        h('span', { class: 'cmd-desc' }, c.description || ''),
      ]));
    }
    panel.style.display = 'block';
  },

  hideSuggestions() {
    const panel = document.getElementById('cmd-suggest');
    if (panel) panel.style.display = 'none';
  },

  applyCommand(c) {
    const ta = document.getElementById('composer-input');
    if (!ta) return;
    ta.value = '/' + c.name + ' ';
    ta.focus();
    this.hideSuggestions();
    this.handleInput({ target: ta });
  },

  handleFocus(e) {
    setTimeout(() => {
      e.target.scrollIntoView({ block: 'end', behavior: 'smooth' });
      this.scrollToBottom(true);
    }, 300);
  },

  handleKeydown(e) {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      this.handleSend();
    }
  },

  async handleSend() {
    const ta = document.getElementById('composer-input');
    if (!ta || this.uploading > 0) return;
    const text = ta.value.trim();
    const paths = this.attachments.filter((a) => a.path).map((a) => a.path);
    if (!text && paths.length === 0) return;
    // omp's read tool decodes images natively; reference the uploaded
    // file paths in the message body.
    let body = text;
    if (paths.length > 0) {
      const list = paths.join('\n');
      body = text ? text + '\n\n' + list : 'Attached image' + (paths.length > 1 ? 's' : '') + ':\n' + list;
    }
    ta.value = '';
    ta.style.height = 'auto';
    this.attachments = [];
    this.renderAttachments();
    this.hideSuggestions();
    this.updateSendState();
    try {
      await api.sendText(this.paneId, body);
      this.scrollToBottom(true);
      this.load();
    } catch (err) {
      showToast('Failed to send message');
    }
  },

  handleInterrupt() {
    if (!confirm('Interrupt agent?')) return;
    api.sendKeys(this.paneId, ['Escape']).catch(() => {
      showToast('Failed to interrupt');
    });
  },
};

export { SessionView };
