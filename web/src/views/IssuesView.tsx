import useSWR from "swr";
import { listIssues } from "../graphql";
import { IssueRow } from "../components/IssueRow";

type Scope = "me" | "active" | "backlog";

export function IssuesView({ scope }: { scope: Scope }) {
  const { data: issues = [] } = useSWR(["issues", scope], () => listIssues());
  const filtered = issues.filter((i) => {
    if (scope === "active") return ["Todo", "In Progress", "In Review"].includes(i.state.name);
    if (scope === "backlog") return ["Backlog"].includes(i.state.name);
    if (scope === "me") return i.assignee?.email !== undefined; // crude
    return true;
  });
  // Group by state
  const groups = new Map<string, typeof filtered>();
  for (const i of filtered) {
    const arr = groups.get(i.state.name) ?? [];
    arr.push(i);
    groups.set(i.state.name, arr);
  }
  return (
    <div className="flex flex-col">
      {[...groups.entries()].map(([name, list]) => (
        <section key={name} className="border-b border-border">
          <header className="px-4 h-9 flex items-center gap-2 bg-surface/60 sticky top-0 z-10">
            <span className="text-[13px] font-medium">{name}</span>
            <span className="text-2xs text-muted">{list.length}</span>
          </header>
          <div>
            {list.map((i) => (
              <IssueRow key={i.id} issue={i} />
            ))}
          </div>
        </section>
      ))}
      {filtered.length === 0 && (
        <div className="px-6 py-12 text-subtle text-[13px]">No issues here.</div>
      )}
    </div>
  );
}
