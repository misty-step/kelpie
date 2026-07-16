// iOS keyboard fix — measures the on-screen keyboard via visualViewport and
// exposes it as --kb-offset. CSS shrinks #app by that amount so the layout
// (transcript, ask box, composer) all stay inside the visible viewport —
// nothing is ever hidden behind the keyboard. Fully feature-detected.
const viewportFix = {
  ready: false,

  // getActiveView returns the mounted view (router.current); used to pin the
  // transcript to the bottom while the keyboard animates in.
  setup(getActiveView) {
    if (this.ready) return;
    const vv = window.visualViewport;
    if (!vv) return;
    this.ready = true;
    const update = () => {
      // Keyboard height = layout viewport minus visual viewport. Do NOT
      // subtract offsetTop here: when Safari pans the page to reveal the
      // focused input, offsetTop > 0 would understate the keyboard and the
      // composer would sit behind it (the bug where you had to drag up).
      const kb = Math.max(0, Math.round(window.innerHeight - vv.height));
      const top = Math.round(vv.offsetTop);
      document.documentElement.style.setProperty('--kb-offset', kb + 'px');
      // Glue #app to the visible area: if Safari panned the layout viewport,
      // translate the app down by the same amount (CSS reads --vv-top).
      document.documentElement.style.setProperty('--vv-top', top + 'px');
      document.documentElement.classList.toggle('kb-open', kb > 60);
      if (kb > 0) window.scrollTo(0, 0);
      if (kb > 0 && getActiveView) {
        const view = getActiveView();
        if (view && typeof view.scrollToBottom === 'function') view.scrollToBottom(true);
      }
    };
    vv.addEventListener('resize', update);
    vv.addEventListener('scroll', update);
    // iOS settles the keyboard animation ~250-600ms after focus, sometimes
    // without a final resize event; re-measure through the animation window.
    const settle = () => { for (const t of [50, 250, 500, 900]) setTimeout(update, t); };
    window.addEventListener('focusin', settle);
    window.addEventListener('focusout', settle);
  },
};

export { viewportFix };
