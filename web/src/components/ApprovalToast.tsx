import { useEffect, useState } from "react";
import { useEventStream } from "../hooks/useEventStream";
import { symphonyApiUrl } from "../symphonyApi";

type Pending = { approval_id: string; issue_id: string; tool: string; input: unknown };

export function ApprovalToast() {
  const [pending, setPending] = useState<Pending[]>([]);
  const events = useEventStream();

  useEffect(() => {
    const last = events[events.length - 1];
    if (!last) return;
    if (last.kind === "approval_request") {
      setPending((p) =>
        p.some((x) => x.approval_id === last.approval_id)
          ? p
          : [...p, { approval_id: last.approval_id, issue_id: last.issue_id, tool: last.tool, input: last.input }]
      );
    } else if (last.kind === "approval_decision") {
      setPending((p) => p.filter((x) => x.approval_id !== last.approval_id));
    }
  }, [events]);

  async function decide(id: string, allow: boolean) {
    await fetch(symphonyApiUrl(`/api/v1/approvals/${id}`), {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ allow, reason: allow ? "operator" : "denied" }),
    });
    setPending((p) => p.filter((x) => x.approval_id !== id));
  }

  if (pending.length === 0) return null;

  return (
    <div className="fixed bottom-4 right-4 z-50 flex flex-col gap-2">
      {pending.map((p) => (
        <div
          key={p.approval_id}
          className="w-80 rounded-lg border border-zinc-700 bg-zinc-900 p-3 shadow-lg"
        >
          <div className="text-xs text-zinc-400">{p.issue_id}</div>
          <div className="text-sm font-medium text-zinc-100">{p.tool}</div>
          <pre className="mt-1 max-h-24 overflow-auto rounded bg-zinc-950 p-2 text-xs text-zinc-300">
            {JSON.stringify(p.input, null, 2)}
          </pre>
          <div className="mt-2 flex gap-2">
            <button
              onClick={() => decide(p.approval_id, true)}
              className="flex-1 rounded bg-emerald-600 px-2 py-1 text-sm text-white hover:bg-emerald-500"
            >
              Approve
            </button>
            <button
              onClick={() => decide(p.approval_id, false)}
              className="flex-1 rounded bg-rose-600 px-2 py-1 text-sm text-white hover:bg-rose-500"
            >
              Deny
            </button>
          </div>
        </div>
      ))}
    </div>
  );
}
