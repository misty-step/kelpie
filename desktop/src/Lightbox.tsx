import { useEffect, useRef, useState } from "react";
import { Minus, Plus, X } from "lucide-react";

// Full-screen zoom modal for diagrams and images.
// - mode "vector": resize the content box (SVG stays crisp; no CSS scale blur)
// - mode "raster": CSS transform scale (photos / bitmaps)
// Wheel zooms toward the cursor; drag pans when scaled up. Esc closes.

const MIN_SCALE = 0.5;
const MAX_SCALE = 6;
const STEP = 0.25;
const DEFAULT_VECTOR = 1.6;
const DEFAULT_RASTER = 1.75;

function clamp(n: number): number {
  return Math.min(MAX_SCALE, Math.max(MIN_SCALE, n));
}

export function Lightbox({
  open,
  onClose,
  label,
  children,
  mode = "raster",
}: {
  open: boolean;
  onClose: () => void;
  label: string;
  children: React.ReactNode;
  /** vector = resize box (sharp SVG); raster = CSS scale (images) */
  mode?: "vector" | "raster";
}) {
  const defaultScale = mode === "vector" ? DEFAULT_VECTOR : DEFAULT_RASTER;
  const [scale, setScale] = useState(defaultScale);
  const [tx, setTx] = useState(0);
  const [ty, setTy] = useState(0);
  const dragRef = useRef<{ x: number; y: number; tx: number; ty: number } | null>(null);

  useEffect(() => {
    if (!open) return;
    setScale(defaultScale);
    setTx(0);
    setTy(0);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
      if (e.key === "+" || e.key === "=") setScale((s) => clamp(s + STEP));
      if (e.key === "-" || e.key === "_") setScale((s) => clamp(s - STEP));
      if (e.key === "0") {
        setScale(defaultScale);
        setTx(0);
        setTy(0);
      }
    };
    window.addEventListener("keydown", onKey);
    const prevOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      window.removeEventListener("keydown", onKey);
      document.body.style.overflow = prevOverflow;
    };
  }, [open, onClose, defaultScale]);

  if (!open) return null;

  const onWheel = (e: React.WheelEvent) => {
    e.preventDefault();
    const delta = e.deltaY > 0 ? -STEP : STEP;
    setScale((s) => clamp(Number((s + delta).toFixed(2))));
  };

  const onPointerDown = (e: React.PointerEvent) => {
    if (e.button !== 0) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    dragRef.current = { x: e.clientX, y: e.clientY, tx, ty };
  };
  const onPointerMove = (e: React.PointerEvent) => {
    const drag = dragRef.current;
    if (!drag) return;
    setTx(drag.tx + (e.clientX - drag.x));
    setTy(drag.ty + (e.clientY - drag.y));
  };
  const onPointerUp = () => {
    dragRef.current = null;
  };

  // Vector mode: pan with translate only; scale via CSS var so SVG reflows
  // at real size (no transform-scale rasterization). Raster mode keeps
  // classic CSS scale for bitmaps.
  const contentStyle: React.CSSProperties =
    mode === "vector"
      ? ({
          transform: `translate(${tx}px, ${ty}px)`,
          ["--zoom-scale" as string]: String(scale),
        } as React.CSSProperties)
      : {
          transform: `translate(${tx}px, ${ty}px) scale(${scale})`,
        };

  return (
    <div className="zoom-overlay" onClick={onClose} role="presentation">
      <div
        className="zoom-chrome"
        role="dialog"
        aria-modal="true"
        aria-label={label}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="zoom-toolbar">
          <span className="zoom-label">{label}</span>
          <div className="zoom-controls">
            <button
              className="zoom-btn"
              title="Zoom out (−)"
              onClick={() => setScale((s) => clamp(s - STEP))}
            >
              <Minus size={14} />
            </button>
            <button
              className="zoom-btn zoom-pct"
              title="Reset zoom (0)"
              onClick={() => {
                setScale(defaultScale);
                setTx(0);
                setTy(0);
              }}
            >
              {Math.round(scale * 100)}%
            </button>
            <button
              className="zoom-btn"
              title="Zoom in (+)"
              onClick={() => setScale((s) => clamp(s + STEP))}
            >
              <Plus size={14} />
            </button>
            <button className="zoom-btn zoom-close" onClick={onClose} title="Close (Esc)">
              <X size={15} />
            </button>
          </div>
        </div>
        <div
          className={`zoom-stage${scale > 1.01 ? " panning" : ""}${mode === "vector" ? " vector" : ""}`}
          onWheel={onWheel}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerCancel={onPointerUp}
        >
          <div className={`zoom-content${mode === "vector" ? " vector" : ""}`} style={contentStyle}>
            {children}
          </div>
        </div>
        <div className="zoom-hint">Scroll to zoom · drag to pan · Esc closes</div>
      </div>
    </div>
  );
}
