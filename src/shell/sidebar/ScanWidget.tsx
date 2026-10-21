/**
 * ScanWidget — the sidebar's live scan card (the reference design):
 * progress ring with the percentage inside, "Deep scanning…" title,
 * found-files line, and the Stop action (plus the pause/resume verb).
 */
import { formatBytes, formatCountFull } from "../../lib/format";
import { scanActive, useScanStore } from "../../state/scan";
import { useViewStore } from "../../state/view";
import { ProgressRing } from "../../components/ui";
import { PauseIcon, PlayIcon, StopIcon } from "../icons";
import type { ScanProgress } from "../../types";

function phaseTitle(p: ScanProgress): string {
  switch (p.phase) {
    case "preparing":
      return "Preparing scan…";
    case "quick":
      return "Quick scanning…";
    case "deep":
      return "Deep scanning…";
    case "organizing":
      return "Organizing files…";
    case "paused":
      return "Scan paused";
    default:
      return "Scanning…";
  }
}

export function ScanWidget() {
  const progress = useScanStore((s) => s.progress);
  const pause = useScanStore((s) => s.pause);
  const resume = useScanStore((s) => s.resume);
  const stop = useScanStore((s) => s.stop);
  const showToast = useViewStore((s) => s.showToast);
  const active = scanActive(progress);
  if (!active || !progress) return null;
  const paused = progress.phase === "paused";

  const onPauseResume = () => {
    if (paused) {
      void resume();
      showToast("Scan resumed", "check");
    } else {
      void pause();
      showToast("Scan paused — the core holds its position", "info");
    }
  };

  return (
    <div className="dw-scan-widget" role="status" aria-label="Scan progress">
      <div className="dw-scan-widget-top">
        <ProgressRing percent={progress.percent} paused={paused} size={54} />
        <div className="dw-scan-widget-body">
          <div className="dw-scan-widget-title">{phaseTitle(progress)}</div>
          <div className="dw-scan-widget-stats dw-num">
            Found {formatCountFull(progress.stats.filesFound)} files
            <br />
            ({formatBytes(progress.stats.bytesFound)})
          </div>
        </div>
      </div>
      <div className="dw-scan-widget-actions">
        <button
          type="button"
          className="dw-ghost-button"
          onClick={onPauseResume}
          aria-label={paused ? "Resume scan" : "Pause scan"}
        >
          {paused ? <PlayIcon size={12} /> : <PauseIcon size={12} />}
          {paused ? "Resume" : "Pause"}
        </button>
        <button
          type="button"
          className="dw-ghost-button dw-stop"
          onClick={() => void stop()}
          aria-label="Stop scan"
        >
          <StopIcon size={11} />
          Stop
        </button>
      </div>
    </div>
  );
}
