import { useEffect, useState } from "react";
import { ShieldAlert } from "lucide-react";
import { useApp } from "../store/app";
import { api, errorMessage } from "../lib/api";
import { prettyJson } from "../lib/format";

/** Shows the oldest pending approval. Nothing sensitive runs until the user decides. */
export function ApprovalDialog() {
  const req = useApp((s) => s.approvals[0]);
  const remaining = useApp((s) => s.approvals.length);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (req) api.showOverlay().catch(() => {});
  }, [req?.id]);

  if (!req) return null;

  const decide = async (decision: "approve" | "approve_session" | "deny") => {
    setBusy(true);
    try {
      await api.resolveApproval(req.id, decision);
    } catch (e) {
      useApp.getState().toast("error", errorMessage(e));
    } finally {
      useApp.getState().removeApproval(req.id);
      setBusy(false);
    }
  };

  return (
    <div
      className="modal-backdrop"
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.preventDefault();
          decide("deny");
        }
      }}
    >
      <div className="modal" role="alertdialog" aria-labelledby="appr-title" aria-describedby="appr-desc">
        <div className="row" style={{ marginBottom: 8 }}>
          <ShieldAlert size={20} className={`risk-${req.risk}`} />
          <h3 id="appr-title">Allow this action?</h3>
          <span className="spacer" />
          <span className={`badge ${req.risk === "high" ? "err" : req.risk === "medium" ? "warn" : "ok"}`}>{req.risk} risk</span>
        </div>
        <p id="appr-desc" style={{ margin: "4px 0 6px", fontWeight: 500 }} className="selectable">{req.description}</p>
        <div className="small muted">
          Tool <span className="mono">{req.tool}</span> · access: {req.access} · requested by {req.source === "task" ? "an automation task" : "the assistant"}
          {remaining > 1 && ` · ${remaining - 1} more waiting`}
        </div>
        <details>
          <summary className="small muted" style={{ cursor: "pointer", marginTop: 8 }}>Exact input</summary>
          <pre>{prettyJson(req.input)}</pre>
        </details>
        <div className="row" style={{ marginTop: 14, justifyContent: "flex-end", flexWrap: "wrap" }}>
          <button className="btn" disabled={busy} onClick={() => decide("deny")} autoFocus>Deny</button>
          {req.allow_session && (
            <button className="btn" disabled={busy} onClick={() => decide("approve_session")} title="Don't ask again for this tool until Windows AI restarts">
              Allow for this session
            </button>
          )}
          <button className="btn primary" disabled={busy} onClick={() => decide("approve")}>Allow once</button>
        </div>
      </div>
    </div>
  );
}
