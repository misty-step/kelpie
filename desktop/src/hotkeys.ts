/** Global keyboard helpers for Kelpie desktop. */

export function mod(e: KeyboardEvent): boolean {
  return e.metaKey || e.ctrlKey;
}

/** True when the event target is an editable field (typing should win). */
export function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  const tag = target.tagName;
  if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return true;
  return Boolean(target.closest('[contenteditable="true"], [role="textbox"]'));
}

/** Digit 1–9 from key/code, else null. */
export function digitFromEvent(e: KeyboardEvent): number | null {
  if (e.code.startsWith("Digit")) {
    const n = Number(e.code.slice(5));
    return n >= 1 && n <= 9 ? n : null;
  }
  if (e.code.startsWith("Numpad")) {
    const n = Number(e.code.slice(6));
    return n >= 1 && n <= 9 ? n : null;
  }
  const n = Number(e.key);
  return n >= 1 && n <= 9 ? n : null;
}
