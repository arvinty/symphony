import useSWR from "swr";
import { listIssues, listStates, updateIssueState } from "../graphql";
import { Link } from "react-router-dom";

export function BoardView() {
  const { data: issues = [], mutate } = useSWR("issues_board", () => listIssues());
  const { data: states = [] } = useSWR("states", listStates);
  const ordered = [...states].sort((a, b) => a.position - b.position);
  return (
    <div className="flex gap-3 p-4 h-full overflow-x-auto">
      {ordered.map((s) => {
        const list = issues.filter((i) => i.state.id === s.id);
        return (
          <div key={s.id} className="w-[300px] shrink-0 flex flex-col bg-surface rounded-lg border border-border">
            <div className="h-9 flex items-center gap-2 px-3 border-b border-border">
              <span
                className="w-2.5 h-2.5 rounded-full"
                style={{ background: s.color ?? "#5e6ad2" }}
              />
              <span className="text-[13px] font-medium">{s.name}</span>
              <span className="ml-auto text-2xs text-muted">{list.length}</span>
            </div>
            <div className="p-2 flex flex-col gap-2 overflow-auto"
              onDragOver={(e) => e.preventDefault()}
              onDrop={async (e) => {
                const id = e.dataTransfer.getData("text/issue");
                if (id) {
                  await updateIssueState(id, s.id);
                  mutate();
                }
              }}
            >
              {list.map((i) => (
                <Link
                  key={i.id}
                  to={`/issue/${i.identifier}`}
                  draggable
                  onDragStart={(e) => e.dataTransfer.setData("text/issue", i.id)}
                  className="rounded-md border border-border bg-elevated p-2 hover:border-accent/60"
                >
                  <div className="text-2xs text-muted font-mono mb-1">{i.identifier}</div>
                  <div className="text-[13px] leading-snug line-clamp-2">{i.title}</div>
                  <div className="flex gap-1 mt-2">
                    {i.labels.nodes.slice(0, 2).map((l) => (
                      <span key={l.id} className="pill" style={{ borderColor: l.color ?? undefined }}>
                        {l.name}
                      </span>
                    ))}
                  </div>
                </Link>
              ))}
            </div>
          </div>
        );
      })}
    </div>
  );
}
