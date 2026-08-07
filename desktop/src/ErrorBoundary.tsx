import { Component, type ReactNode } from "react";

// Safety net: a render error anywhere in the content subtree must not take
// down the titlebar and its window controls. The boundary sits around the
// body/modals in App, so the toolbar always stays reachable (including the
// real quit paths).

interface Props {
  children: ReactNode;
}
interface State {
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  render() {
    if (this.state.error) {
      return (
        <div className="crash-screen">
          <div className="crash-title">Something went wrong in this view</div>
          <div className="crash-detail">{String(this.state.error?.message ?? this.state.error)}</div>
          <button className="crash-reload" onClick={() => location.reload()}>
            Reload kelpie
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}
