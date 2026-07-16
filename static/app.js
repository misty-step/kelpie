// ============================================================
// kelpie — Signal Deck: fleet operations console for omp agents
// Vanilla ES-module SPA, no build step. Hash routing:
//   #/                inbox (fleet triage)
//   #/pane/{pane_id}  agent session (transcript + composer)
//   #/term/{pane_id}  raw terminal (screen + key row)
//
// Module map (static/js/):
//   dom.js       DOM + formatting primitives
//   icons.js     Lucide icons + deterministic workspace identity
//   markdown.js  safe markdown subset for assistant text
//   api.js       bridge API client + command/model caches
//   state.js     shared fleet cache (panes / workspaces / tabs)
//   overlay.js   toast + bottom-sheet primitives
//   sse.js       event stream with reconnect backoff
//   viewport.js  iOS keyboard / visualViewport handling
//   tabstrip.js  shared workspace tab strip
//   views/       inbox, session, terminal
// ============================================================
import { sse } from './js/sse.js';
import { viewportFix } from './js/viewport.js';
import { InboxView } from './js/views/inbox.js';
import { SessionView } from './js/views/session.js';
import { TermView } from './js/views/term.js';

const router = {
  current: null,

  start() {
    window.addEventListener('hashchange', () => this.resolve());
    this.resolve();
  },

  resolve() {
    const hash = location.hash || '#/';
    if (this.current && this.current.unmount) this.current.unmount();

    const paneMatch = hash.match(/^#\/pane\/([^/]+)/);
    if (paneMatch) {
      const paneId = decodeURIComponent(paneMatch[1]);
      this.current = SessionView;
      SessionView.mount(paneId);
      return;
    }

    const termMatch = hash.match(/^#\/term\/([^/]+)/);
    if (termMatch) {
      const paneId = decodeURIComponent(termMatch[1]);
      this.current = TermView;
      TermView.mount(paneId);
      return;
    }

    this.current = InboxView;
    InboxView.mount();
  },
};

document.addEventListener('visibilitychange', () => {
  if (document.visibilityState === 'visible') {
    if (router.current && router.current.onVisible) router.current.onVisible();
  }
});

sse.connect();
router.start();
viewportFix.setup(() => router.current);
