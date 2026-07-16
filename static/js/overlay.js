import { h } from './dom.js';

// ------------------------------------------------------------
// Toast
// ------------------------------------------------------------
let toastTimer = null;
function showToast(msg, kind) {
  let el = document.getElementById('kelpie-toast');
  if (!el) {
    el = h('div', { class: 'toast', id: 'kelpie-toast' });
    document.body.appendChild(el);
  }
  el.className = 'toast' + (kind === 'info' ? ' info' : '');
  el.textContent = msg;
  el.style.display = 'block';
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { el.style.display = 'none'; }, 3200);
}

// ------------------------------------------------------------
// Bottom sheet — generic action/picker surface. Tap the scrim to
// dismiss. The builder receives (sheet, close).
// ------------------------------------------------------------
function openSheet(build) {
  const overlay = h('div', { class: 'sheet-overlay' });
  const sheet = h('div', { class: 'sheet' });
  overlay.appendChild(sheet);
  const close = () => overlay.remove();
  overlay.addEventListener('click', (e) => { if (e.target === overlay) close(); });
  build(sheet, close);
  document.body.appendChild(overlay);
  return close;
}

export { showToast, openSheet };
