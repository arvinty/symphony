import { useLocation } from "react-router-dom";

export function TopBar({ onOpenCmd }: { onOpenCmd: () => void }) {
  const loc = useLocation();
  const title = niceTitle(loc.pathname);
  return (
    <div className="h-12 border-b border-border flex items-center px-4 gap-3 bg-surface/40 backdrop-blur">
      <div className="text-[13px] font-medium">{title}</div>
      <div className="flex-1" />
      <button
        onClick={onOpenCmd}
        className="text-2xs text-subtle border border-border rounded-md px-2 h-7 hover:bg-elevated"
      >
        ⌘K
      </button>
      <button className="icon-btn">⚙</button>
    </div>
  );
}

function niceTitle(path: string): string {
  if (path.startsWith("/inbox")) return "Inbox";
  if (path.startsWith("/my-issues")) return "My issues";
  if (path.startsWith("/active")) return "Active";
  if (path.startsWith("/backlog")) return "Backlog";
  if (path.startsWith("/board")) return "Board";
  if (path.startsWith("/project/")) return "Project";
  if (path.startsWith("/issue/")) return "Issue";
  return "Linear";
}
