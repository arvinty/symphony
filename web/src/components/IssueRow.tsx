import { Link } from "react-router-dom";
import type { Issue } from "../graphql";

export function IssueRow({ issue }: { issue: Issue }) {
  const dot = issue.state.color ?? "#5e6ad2";
  return (
    <Link to={`/issue/${issue.identifier}`} className="row group">
      <PriorityIcon p={issue.priority ?? 0} />
      <span
        className="w-2.5 h-2.5 rounded-full shrink-0"
        title={issue.state.name}
        style={{ background: dot }}
      />
      <span className="text-2xs text-muted w-16 shrink-0 font-mono">{issue.identifier}</span>
      <span className="truncate flex-1 text-text/95">{issue.title}</span>
      {issue.labels.nodes.slice(0, 3).map((l) => (
        <span key={l.id} className="pill" style={{ borderColor: l.color ?? undefined }}>
          <span
            className="w-1.5 h-1.5 rounded-full"
            style={{ background: l.color ?? "#62646a" }}
          />
          {l.name}
        </span>
      ))}
      {issue.assignee ? (
        <span className="w-5 h-5 rounded-full bg-accent/40 flex items-center justify-center text-[10px] uppercase">
          {issue.assignee.displayName.slice(0, 1)}
        </span>
      ) : (
        <span className="w-5 h-5 rounded-full border border-dashed border-border/60" />
      )}
      <span className="text-2xs text-muted w-12 text-right shrink-0">
        {new Date(issue.updatedAt).toLocaleDateString(undefined, { month: "short", day: "numeric" })}
      </span>
    </Link>
  );
}

function PriorityIcon({ p }: { p: number }) {
  // Linear-style 4-bar priority indicator
  const bars = [1, 2, 3].map((i) => {
    const filled = (p === 1 && i <= 3) || (p === 2 && i <= 2) || (p === 3 && i <= 1);
    return (
      <span
        key={i}
        className={`w-[3px] inline-block rounded-sm ${filled ? "bg-text" : "bg-border"}`}
        style={{ height: 4 + i * 3 }}
      />
    );
  });
  if (p === 0)
    return (
      <span className="w-4 h-4 inline-flex items-center justify-center text-muted">—</span>
    );
  return <span className="w-4 h-4 inline-flex items-end gap-[2px]">{bars}</span>;
}
