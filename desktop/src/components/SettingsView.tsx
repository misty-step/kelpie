import { useEffect, useState } from "react";
import { Sliders, Terminal, RotateCcw, Check, Command, AlertCircle, Type, Minus, Plus } from "lucide-react";
import { win } from "../windowApi";
import type { HerdrStatus } from "../types";
import {
  TYPE_SCALE_DEFAULT,
  TYPE_SCALE_MAX,
  TYPE_SCALE_MIN,
  TYPE_SCALE_STEP,
  applyTypeScale,
  getSavedTypeScale,
  type TypeScalePx,
} from "../typeScale";

const OPACITY_STORAGE_KEY = "kelpie.opacity";
const DEFAULT_OPACITY = 100;

export function getSavedOpacity(): number {
  const saved = localStorage.getItem(OPACITY_STORAGE_KEY);
  if (!saved) return DEFAULT_OPACITY;
  const parsed = parseInt(saved, 10);
  return isNaN(parsed) ? DEFAULT_OPACITY : Math.max(20, Math.min(100, parsed));
}

export async function applyOpacitySetting(percent: number): Promise<void> {
  localStorage.setItem(OPACITY_STORAGE_KEY, percent.toString());
  const w = win();
  if (w) {
    await w.setOpacity(percent / 100);
  }
}

export function SettingsView({ herdr }: { herdr: HerdrStatus; onClose?: () => void }) {
  const [opacity, setOpacity] = useState<number>(getSavedOpacity());
  const [typeScale, setTypeScale] = useState<TypeScalePx>(getSavedTypeScale());
  const [savedBadge, setSavedBadge] = useState<boolean>(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  useEffect(() => {
    applyOpacitySetting(opacity).catch((err: unknown) => {
      console.error("[SettingsView] Failed to apply opacity:", err);
      setErrorMsg(err instanceof Error ? err.message : String(err));
    });
  }, [opacity]);

  useEffect(() => {
    const onScale = (e: Event) => {
      const detail = (e as CustomEvent<number>).detail;
      if (typeof detail === "number") setTypeScale(detail);
    };
    window.addEventListener("kelpie:type-scale", onScale);
    return () => window.removeEventListener("kelpie:type-scale", onScale);
  }, []);

  const flashSaved = () => {
    setSavedBadge(true);
    setTimeout(() => setSavedBadge(false), 1500);
  };

  const handleOpacityChange = (newVal: number) => {
    setErrorMsg(null);
    setOpacity(newVal);
    applyOpacitySetting(newVal)
      .then(flashSaved)
      .catch((err: unknown) => {
        setErrorMsg(err instanceof Error ? err.message : String(err));
      });
  };

  const handleTypeScaleChange = (px: number) => {
    setTypeScale(applyTypeScale(px));
    flashSaved();
  };

  const handleResetOpacity = () => {
    handleOpacityChange(DEFAULT_OPACITY);
  };

  const handleResetTypeScale = () => {
    handleTypeScaleChange(TYPE_SCALE_DEFAULT);
  };

  const presets = [100, 90, 80, 70, 50];
  const typePresets = [13, 15, 17, 19];
  return (
    <div className="settings-view">
      <header className="settings-header">
        <div className="settings-title-group">
          <h1>Settings</h1>
          <p className="settings-subtitle">
            Configure application appearance, window opacity, and connection status.
          </p>
        </div>
        {savedBadge && (
          <span className="settings-saved-badge">
            <Check size={12} /> Saved
          </span>
        )}
        {errorMsg && (
          <span className="settings-error-badge" title={errorMsg}>
            <AlertCircle size={12} /> {errorMsg}
          </span>
        )}
      </header>

      <div className="settings-body">
        {/* --- Window & Display Section --- */}
        <section className="settings-card">
          <div className="settings-card-header">
            <Sliders size={18} className="settings-icon" />
            <div>
              <h2>Window & Display</h2>
              <p>Customize opacity and transparency of the desktop window</p>
            </div>
          </div>

          <div className="settings-group">
            <div className="settings-row">
              <div className="settings-label-col">
                <label htmlFor="opacity-slider" className="settings-label">
                  Window Opacity
                </label>
                <span className="settings-desc">
                  Adjust the transparency of the native application window.
                </span>
              </div>
              <div className="settings-value-col">
                <span className="settings-pill">{opacity}%</span>
              </div>
            </div>

            <div className="settings-slider-row">
              <input
                id="opacity-slider"
                type="range"
                min="20"
                max="100"
                step="5"
                value={opacity}
                onChange={(e) => handleOpacityChange(parseInt(e.target.value, 10))}
                className="settings-slider"
              />
            </div>

            <div className="settings-presets-row">
              <span className="settings-presets-label">Presets:</span>
              <div className="settings-presets-btns">
                {presets.map((p) => (
                  <button
                    key={p}
                    type="button"
                    className={`settings-chip ${opacity === p ? "active" : ""}`}
                    onClick={() => handleOpacityChange(p)}
                  >
                    {p === 100 ? "100% (Opaque)" : `${p}%`}
                  </button>
                ))}
                {opacity !== DEFAULT_OPACITY && (
                  <button
                    type="button"
                    className="settings-chip reset"
                    onClick={handleResetOpacity}
                    title="Reset to 100%"
                  >
                    <RotateCcw size={12} /> Reset
                  </button>
                )}
              </div>
            </div>
          </div>
        </section>

        <section className="settings-card">
          <div className="settings-card-header">
            <Type size={18} className="settings-icon" />
            <div>
              <h2>Typography</h2>
              <p>Chat body size. Chrome stays compact; transcript scales with this value.</p>
            </div>
          </div>

          <div className="settings-group">
            <div className="settings-row">
              <div className="settings-label-col">
                <label htmlFor="type-scale-slider" className="settings-label">
                  Text size
                </label>
                <span className="settings-desc">
                  Default 15px. Also: Ctrl+Plus / Ctrl+Minus / Ctrl+0.
                </span>
              </div>
              <div className="settings-value-col">
                <span className="settings-pill">{typeScale}px</span>
              </div>
            </div>

            <div className="settings-type-row">
              <button
                type="button"
                className="settings-type-btn"
                aria-label="Decrease text size"
                disabled={typeScale <= TYPE_SCALE_MIN}
                onClick={() => handleTypeScaleChange(typeScale - TYPE_SCALE_STEP)}
              >
                <Minus size={14} />
              </button>
              <input
                id="type-scale-slider"
                type="range"
                min={TYPE_SCALE_MIN}
                max={TYPE_SCALE_MAX}
                step={TYPE_SCALE_STEP}
                value={typeScale}
                onChange={(e) => handleTypeScaleChange(parseInt(e.target.value, 10))}
                className="settings-slider"
              />
              <button
                type="button"
                className="settings-type-btn"
                aria-label="Increase text size"
                disabled={typeScale >= TYPE_SCALE_MAX}
                onClick={() => handleTypeScaleChange(typeScale + TYPE_SCALE_STEP)}
              >
                <Plus size={14} />
              </button>
            </div>

            <div className="settings-presets-row">
              <span className="settings-presets-label">Presets:</span>
              <div className="settings-presets-btns">
                {typePresets.map((p) => (
                  <button
                    key={p}
                    type="button"
                    className={`settings-chip ${typeScale === p ? "active" : ""}`}
                    onClick={() => handleTypeScaleChange(p)}
                  >
                    {p === TYPE_SCALE_DEFAULT ? `${p}px (Default)` : `${p}px`}
                  </button>
                ))}
                {typeScale !== TYPE_SCALE_DEFAULT && (
                  <button
                    type="button"
                    className="settings-chip reset"
                    onClick={handleResetTypeScale}
                    title={`Reset to ${TYPE_SCALE_DEFAULT}px`}
                  >
                    <RotateCcw size={12} /> Reset
                  </button>
                )}
              </div>
            </div>

            <p className="settings-type-sample md" aria-hidden="true">
              The agent will answer in this size. Headings, lists, and code scale with it.
            </p>
          </div>
        </section>

        {/* --- Fleet & Herdr Status Section --- */}
        <section className="settings-card">
          <div className="settings-card-header">
            <Terminal size={18} className="settings-icon" />
            <div>
              <h2>Fleet & Control Plane</h2>
              <p>In-process herdr socket client and omp transcript engine</p>
            </div>
          </div>

          <div className="settings-group">
            <div className="settings-row">
              <div className="settings-label-col">
                <span className="settings-label">Herdr Socket Status</span>
                <span className="settings-desc">UNIX domain socket connection to herdr</span>
              </div>
              <div className="settings-value-col">
                <span className={`status-chip ${herdr.ok ? "working" : "blocked"}`}>
                  <span className={`status-dot tiny ${herdr.ok ? "working" : "blocked"}`} />
                  {herdr.ok ? "connected" : "disconnected"}
                </span>
              </div>
            </div>

            <div className="settings-row">
              <div className="settings-label-col">
                <span className="settings-label">Socket Endpoint</span>
              </div>
              <div className="settings-value-col">
                <code className="settings-code">/tmp/herdr.sock</code>
              </div>
            </div>

            <div className="settings-row">
              <div className="settings-label-col">
                <span className="settings-label">Poll Frequency</span>
              </div>
              <div className="settings-value-col">
                <code className="settings-code">600 ms</code>
              </div>
            </div>
          </div>
        </section>

        {/* --- Keyboard Shortcuts Section --- */}
        <section className="settings-card">
          <div className="settings-card-header">
            <Command size={18} className="settings-icon" />
            <div>
              <h2>Keyboard Shortcuts</h2>
              <p>Global accelerators and navigation hotkeys</p>
            </div>
          </div>

          <div className="settings-group">
            <div className="settings-row">
              <div className="settings-label-col">
                <span className="settings-label">Command palette</span>
              </div>
              <div className="settings-value-col">
                <kbd className="settings-kbd">Ctrl</kbd> + <kbd className="settings-kbd">K</kbd>
              </div>
            </div>

            <div className="settings-row">
              <div className="settings-label-col">
                <span className="settings-label">Next agent</span>
                <span className="settings-desc">Sidebar order (needs attention first)</span>
              </div>
              <div className="settings-value-col settings-keys">
                <span>
                  <kbd className="settings-kbd">Alt</kbd> + <kbd className="settings-kbd">↓</kbd>
                </span>
                <span className="settings-keys-or">or</span>
                <span>
                  <kbd className="settings-kbd">Alt</kbd> + <kbd className="settings-kbd">J</kbd>
                </span>
              </div>
            </div>

            <div className="settings-row">
              <div className="settings-label-col">
                <span className="settings-label">Previous agent</span>
              </div>
              <div className="settings-value-col settings-keys">
                <span>
                  <kbd className="settings-kbd">Alt</kbd> + <kbd className="settings-kbd">↑</kbd>
                </span>
                <span className="settings-keys-or">or</span>
                <span>
                  <kbd className="settings-kbd">Alt</kbd> + <kbd className="settings-kbd">K</kbd>
                </span>
              </div>
            </div>

            <div className="settings-row">
              <div className="settings-label-col">
                <span className="settings-label">Jump to agent 1–9</span>
              </div>
              <div className="settings-value-col">
                <kbd className="settings-kbd">Ctrl</kbd> + <kbd className="settings-kbd">1</kbd>
                <span className="settings-keys-or">…</span>
                <kbd className="settings-kbd">9</kbd>
              </div>
            </div>

            <div className="settings-row">
              <div className="settings-label-col">
                <span className="settings-label">Next needs attention</span>
              </div>
              <div className="settings-value-col">
                <kbd className="settings-kbd">Ctrl</kbd> + <kbd className="settings-kbd">Shift</kbd> +{" "}
                <kbd className="settings-kbd">A</kbd>
              </div>
            </div>

            <div className="settings-row">
              <div className="settings-label-col">
                <span className="settings-label">Fleet view</span>
              </div>
              <div className="settings-value-col">
                <kbd className="settings-kbd">Ctrl</kbd> + <kbd className="settings-kbd">F</kbd>
                <span className="settings-keys-or">or</span>
                <kbd className="settings-kbd">Esc</kbd>
              </div>
            </div>

            <div className="settings-row">
              <div className="settings-label-col">
                <span className="settings-label">Settings</span>
              </div>
              <div className="settings-value-col">
                <kbd className="settings-kbd">Ctrl</kbd> + <kbd className="settings-kbd">,</kbd>
              </div>
            </div>

            <div className="settings-row">
              <div className="settings-label-col">
                <span className="settings-label">Usage panel</span>
              </div>
              <div className="settings-value-col">
                <kbd className="settings-kbd">Ctrl</kbd> + <kbd className="settings-kbd">U</kbd>
              </div>
            </div>

            <div className="settings-row">
              <div className="settings-label-col">
                <span className="settings-label">Toggle terminal</span>
              </div>
              <div className="settings-value-col">
                <kbd className="settings-kbd">Ctrl</kbd> + <kbd className="settings-kbd">`</kbd>
              </div>
            </div>

            <div className="settings-row">
              <div className="settings-label-col">
                <span className="settings-label">Quit</span>
              </div>
              <div className="settings-value-col">
                <kbd className="settings-kbd">Ctrl</kbd> + <kbd className="settings-kbd">Q</kbd>
              </div>
            </div>

            <div className="settings-row">
              <div className="settings-label-col">
                <span className="settings-label">Larger / smaller / reset text</span>
              </div>
              <div className="settings-value-col settings-keys">
                <span>
                  <kbd className="settings-kbd">Ctrl</kbd> + <kbd className="settings-kbd">+</kbd>
                </span>
                <span className="settings-keys-or">/</span>
                <span>
                  <kbd className="settings-kbd">−</kbd>
                </span>
                <span className="settings-keys-or">/</span>
                <span>
                  <kbd className="settings-kbd">0</kbd>
                </span>
              </div>
            </div>
          </div>
        </section>
      </div>
    </div>
  );
}
