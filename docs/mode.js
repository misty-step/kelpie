/* the mode: light and dark, defaulting to the system, with a toggle.

   The boot — read the persisted choice before first paint so the page
   never flashes the wrong mode — must be inlined in <head>:

   <script>
     try {
       var m = localStorage.getItem('ae-mode');
       if (m === 'dark' || m === 'light') {
         document.documentElement.classList.add(m);
         document.documentElement.style.colorScheme = m;
       }
     } catch (e) {}
   </script>

   The toggle below pins the opposite of the effective scheme. Pinning
   color-scheme keeps UA widgets (scrollbars, form controls) on the
   pinned side even when the OS disagrees. The change itself is quick
   and interruptible: a new click cuts any in-flight transition and
   applies the newest mode immediately. Unsupported view transitions
   get the same quick color ease; reduced motion is instant.
   next-themes consumers: keep attribute="class" and skip this file —
   the classes match. */
(() => {
  const root = document.documentElement;
  let activeTransition = null;
  let easingTimer = 0;
  let runId = 0;
  let targetDark = null;

  const isDark = () =>
    root.classList.contains('dark')
      ? true
      : root.classList.contains('light')
        ? false
        : matchMedia('(prefers-color-scheme: dark)').matches;

  const reducedMode = matchMedia('(prefers-reduced-motion: reduce)');

  const clearAnimation = () => {
    if (activeTransition && activeTransition.skipTransition) {
      activeTransition.skipTransition();
    }
    activeTransition = null;
    if (easingTimer) {
      clearTimeout(easingTimer);
      easingTimer = 0;
    }
    root.classList.remove('ae-vt-mode', 'ae-mode-easing');
  };

  const applyMode = (dark) => {
    root.classList.toggle('dark', dark);
    root.classList.toggle('light', !dark);
    root.style.colorScheme = dark ? 'dark' : 'light';
    try {
      localStorage.setItem('ae-mode', dark ? 'dark' : 'light');
    } catch (e) {}
  };

  document.querySelectorAll('.ae-mode').forEach((btn) => {
    btn.addEventListener('click', () => {
      const nextDark = !(targetDark ?? isDark());
      const id = ++runId;
      targetDark = nextDark;
      const flip = () => {
        if (id !== runId) return;
        applyMode(nextDark);
      };
      clearAnimation();
      if (reducedMode.matches) {
        flip();
      } else if (document.startViewTransition) {
        root.classList.add('ae-vt-mode');
        activeTransition = document.startViewTransition(flip);
        easingTimer = setTimeout(() => {
          if (id !== runId) return;
          root.classList.remove('ae-vt-mode');
          easingTimer = 0;
        }, 180);
        activeTransition.finished.finally(() => {
          if (id !== runId) return;
          root.classList.remove('ae-vt-mode');
          activeTransition = null;
          if (easingTimer) {
            clearTimeout(easingTimer);
            easingTimer = 0;
          }
        });
      } else {
        root.classList.add('ae-mode-easing');
        flip();
        easingTimer = setTimeout(() => {
          if (id !== runId) return;
          root.classList.remove('ae-mode-easing');
          easingTimer = 0;
        }, 180);
      }
    });
  });
})();
