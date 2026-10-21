/**
 * HelpCard — the sidebar's pinned help card (the reference design):
 * "Need help?" copy + the Open Help Center action.
 */
import { ArrowRightIcon, ChatIcon } from "../icons";
import { useViewStore } from "../../state/view";

export function HelpCard() {
  const setPage = useViewStore((s) => s.setPage);
  return (
    <div className="dw-help-card">
      <div className="dw-help-card-art" aria-hidden="true">
        <ChatIcon size={17} strokeWidth={1.8} />
      </div>
      <div>
        <div className="dw-help-card-title">Need help?</div>
        <div className="dw-help-card-sub">Get support or visit our help center.</div>
      </div>
      <button type="button" className="dw-help-card-btn" onClick={() => setPage("help")}>
        Open Help Center
        <ArrowRightIcon size={12} />
      </button>
    </div>
  );
}
