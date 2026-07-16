// DOM + formatting primitives shared by every view.
const app = document.getElementById('app');

// ------------------------------------------------------------
// Small DOM helpers
// ------------------------------------------------------------
function h(tag, attrs, children) {
  const el = document.createElement(tag);
  if (attrs) {
    for (const k in attrs) {
      if (k === 'class') el.className = attrs[k];
      else if (k === 'html') el.innerHTML = attrs[k];
      else if (k.startsWith('on') && typeof attrs[k] === 'function') {
        el.addEventListener(k.slice(2), attrs[k]);
      } else if (attrs[k] !== null && attrs[k] !== undefined) {
        el.setAttribute(k, attrs[k]);
      }
    }
  }
  if (children) {
    for (const c of [].concat(children)) {
      if (c === null || c === undefined) continue;
      el.appendChild(typeof c === 'string' ? document.createTextNode(c) : c);
    }
  }
  return el;
}

function escapeHtml(s) {
  return String(s)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

function clear(el) {
  while (el.firstChild) el.removeChild(el.firstChild);
}

function prefersReducedMotion() {
  return !!(window.matchMedia && window.matchMedia('(prefers-reduced-motion: reduce)').matches);
}

// ------------------------------------------------------------
// Time helpers — defensive against null/invalid timestamps
// ------------------------------------------------------------
function relTime(iso) {
  if (!iso) return '';
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return '';
  const diffMs = Date.now() - t;
  const s = Math.round(diffMs / 1000);
  if (s < 5) return 'now';
  if (s < 60) return s + 's';
  const m = Math.round(s / 60);
  if (m < 60) return m + 'm';
  const hr = Math.round(m / 60);
  if (hr < 24) return hr + 'h';
  const d = Math.round(hr / 24);
  return d + 'd';
}

function basename(p) {
  if (!p) return null;
  const parts = String(p).replace(/\/+$/, '').split('/');
  return parts[parts.length - 1] || p;
}

export { app, h, clear, escapeHtml, prefersReducedMotion, relTime, basename };
