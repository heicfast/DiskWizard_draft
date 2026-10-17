/**
 * App-level error boundary: a crash in any page renders a clean,
 * brand-styled failure card (never a blank window) with the error
 * message and a reload action — the honest-degradation principle.
 */
import { Component, type ErrorInfo, type ReactNode } from "react";
import { BrandTile } from "../brand/BrandMark";

interface Props {
  children: ReactNode;
}

interface State {
  error: Error | null;
}

export class AppErrorBoundary extends Component<Props, State> {
  override state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  override componentDidCatch(error: Error, info: ErrorInfo): void {
    // eslint-disable-next-line no-console -- the console channel is the dev surface
    console.error("[DiskWizard] UI failure:", error, info.componentStack);
  }

  override render(): ReactNode {
    if (!this.state.error) return this.props.children;
    return (
      <div className="dw-crash">
        <div className="dw-crash-card">
          <BrandTile size={40} />
          <h1 className="dw-display">DiskWizard hit a problem</h1>
          <p className="dw-crash-copy">
            The interface encountered an unexpected error and stopped. Your scans and
            recovered data are safe — reloading the window resumes from the last
            saved state.
          </p>
          <pre className="dw-crash-detail">{String(this.state.error.message)}</pre>
          <button type="button" className="dw-ink-button" onClick={() => window.location.reload()}>
            Reload DiskWizard
          </button>
        </div>
      </div>
    );
  }
}
