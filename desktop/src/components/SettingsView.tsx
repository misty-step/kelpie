import { useEffect, useState } from "react";
import { Sliders, Terminal, RotateCcw, Check, Command, AlertCircle } from "lucide-react";
import { win } from "../windowApi";
import type { HerdrStatus } from "../types";

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
  const [savedBadge, setSavedBadge] = useState<boolean>(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  useEffect(() => {
    applyOpacitySetting(opacity).catch((err: unknown) => {
      console.error("[SettingsView] Failed to apply opacity:", err);
      setErrorMsg(err instanceof Error ? err.message : String(err));
    });
  }, [opacity]);

  const handleOpacityChange = (newVal: number) => {
    setErrorMsg(null);
    setOpacity(newVal);
    applyOpacitySetting(newVal)
      .then(() => {
        setSavedBadge(true);
        setTimeout(() => setSavedBadge(false), 1500);
      })
      .catch((err: unknown) => {
        setErrorMsg(err instanceof Error ? err.message : String(err));
      });
  };

  const handleResetOpacity = () => {
    handleOpacityChange(DEFAULT_OPACITY);
  };

  const presets = [100, 90, 80, 70, 50];

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
                <span className="settings-label">Command Palette</span>
              </div>
              <div className="settings-value-col">
                <kbd className="settings-kbd">Ctrl</kbd> + <kbd className="settings-kbd">K</kbd>
              </div>
            </div>

            <div className="settings-row">
              <div className="settings-label-col">
                <span className="settings-label">Quit Application</span>
              </div>
              <div className="settings-value-col">
                <kbd className="settings-kbd">Ctrl</kbd> + <kbd className="settings-kbd">Q</kbd>
              </div>
            </div>

            <div className="settings-row">
              <div className="settings-label-col">
                <span className="settings-label">Close Overlay / Menu</span>
              </div>
              <div className="settings-value-col">
                <kbd className="settings-kbd">Esc</kbd>
              </div>
            </div>
          </div>
        </section>
      </div>
    </div>
  );
}
