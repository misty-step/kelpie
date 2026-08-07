/** Chat / UI type scale. Body size is the control surface; chat sizes ride rem from html. */

export const TYPE_SCALE_KEY = "kelpie.typeScale";
export const TYPE_SCALE_MIN = 12;
export const TYPE_SCALE_MAX = 20;
export const TYPE_SCALE_DEFAULT = 15;
export const TYPE_SCALE_STEP = 1;

export type TypeScalePx = number;

export function clampTypeScale(px: number): TypeScalePx {
  const n = Math.round(px);
  if (Number.isNaN(n)) return TYPE_SCALE_DEFAULT;
  return Math.max(TYPE_SCALE_MIN, Math.min(TYPE_SCALE_MAX, n));
}

export function getSavedTypeScale(): TypeScalePx {
  try {
    const raw = localStorage.getItem(TYPE_SCALE_KEY);
    if (!raw) return TYPE_SCALE_DEFAULT;
    return clampTypeScale(parseInt(raw, 10));
  } catch {
    return TYPE_SCALE_DEFAULT;
  }
}

export function applyTypeScale(px: TypeScalePx): TypeScalePx {
  const size = clampTypeScale(px);
  try {
    localStorage.setItem(TYPE_SCALE_KEY, String(size));
  } catch {
    /* ignore quota */
  }
  document.documentElement.style.setProperty("--font-size", `${size}px`);
  document.documentElement.dataset.typeScale = String(size);
  window.dispatchEvent(new CustomEvent("kelpie:type-scale", { detail: size }));
  return size;
}

export function bumpTypeScale(delta: number): TypeScalePx {
  return applyTypeScale(getSavedTypeScale() + delta * TYPE_SCALE_STEP);
}
