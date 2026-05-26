import { useEffect, useState } from "react";
import { symphonyApiUrl } from "../symphonyApi";

export type OrchestratorEvent =
  | { kind: "agent_event"; issue_id: string; event: unknown }
  | { kind: "tool_call"; issue_id: string; tool: string; input: unknown }
  | { kind: "tool_result"; issue_id: string; tool: string; output: unknown; error?: string }
  | { kind: "approval_request"; issue_id: string; approval_id: string; tool: string; input: unknown }
  | { kind: "approval_decision"; issue_id: string; approval_id: string; allow: boolean; reason?: string }
  | { kind: "vcs_pushed"; issue_id: string; branch: string }
  | { kind: "pr_opened"; issue_id: string; url: string }
  | { kind: "vcs_error"; issue_id: string; stage: string; message: string }
  | { kind: "resync" };

export function useEventStream(issueId?: string): OrchestratorEvent[] {
  const [events, setEvents] = useState<OrchestratorEvent[]>([]);

  useEffect(() => {
    setEvents([]);
    const url = symphonyApiUrl(
      issueId
        ? `/api/v1/events?issue=${encodeURIComponent(issueId)}`
        : "/api/v1/events",
    );
    const es = new EventSource(url);
    es.onmessage = (e) => {
      try {
        const evt = JSON.parse(e.data) as OrchestratorEvent;
        setEvents((prev) => [...prev.slice(-499), evt]);
      } catch {
        // ignore malformed frames
      }
    };
    es.onerror = () => {
      es.close();
    };
    return () => es.close();
  }, [issueId]);

  return events;
}
