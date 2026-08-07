import { useEffect, useState } from "react";
import mermaid from "mermaid";
import { Lightbox } from "./Lightbox";
import type { Theme } from "./theme";

let seq = 0;

const THEME_VARS: Record<Theme, Record<string, string>> = {
  light: {
    primaryColor: "#e8edff",
    primaryTextColor: "#2d2e30",
    primaryBorderColor: "#8891e1",
    lineColor: "#c0c4d0",
    textColor: "#2d2e30",
    background: "#ffffff",
    clusterBkg: "#f7f8fa",
    clusterBorder: "#e0e0e0",
    edgeLabelBackground: "#ffffff",
    fontSize: "13px",
  },
  dark: {
    primaryColor: "#1c2430",
    primaryTextColor: "#e8e6e3",
    primaryBorderColor: "#8891e1",
    lineColor: "#3a4150",
    textColor: "#e8e6e3",
    background: "#1b1e24",
    clusterBkg: "#20242b",
    clusterBorder: "#2c313b",
    edgeLabelBackground: "#1b1e24",
    fontSize: "13px",
  },
};

export function Mermaid({ code, theme }: { code: string; theme: Theme }) {
  const [svg, setSvg] = useState<string | null>(null);
  const [zoom, setZoom] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    const id = `kelpie-m${++seq}`;
    mermaid.initialize({
      startOnLoad: false,
      securityLevel: "strict",
      theme: "base",
      themeVariables: THEME_VARS[theme],
    });
    mermaid
      .render(id, code)
      .then(({ svg }) => {
        if (!cancelled) setSvg(svg);
      })
      .catch((e: unknown) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [code, theme]);

  if (error) {
    return (
      <div className="mermaid-fallback">
        <div className="mermaid-err-label">Diagram failed to render — raw source:</div>
        <pre>{code}</pre>
      </div>
    );
  }

  const markup = { __html: svg ?? "" };
  return (
    <>
      <button
        className="mermaid-trigger"
        title="Click to enlarge"
        aria-label="Enlarge diagram"
        onClick={() => svg && setZoom(true)}
      >
        <span className="mermaid" dangerouslySetInnerHTML={markup} />
      </button>
      <Lightbox open={zoom} onClose={() => setZoom(false)} label="Enlarged diagram" mode="vector">
        <div className="zoom-mermaid" dangerouslySetInnerHTML={markup} />
      </Lightbox>
    </>
  );
}
