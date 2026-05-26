import { useState } from "react";
import useSWR from "swr";
import { createIssue, listIssues, listProjects, listStates, listTeams, updateIssueState } from "../graphql";
import { Link } from "react-router-dom";

export function BoardView() {
  const { data: issues = [], mutate } = useSWR("issues_board", () => listIssues());
  const { data: states = [] } = useSWR("states", listStates);
  const { data: teams = [] } = useSWR("teams", listTeams);
  const { data: projects = [] } = useSWR("projects", listProjects);
  const [draftOpen, setDraftOpen] = useState(false);
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [saving, setSaving] = useState(false);
  const ordered = [...states].sort((a, b) => a.position - b.position);
  const todo = states.find((s) => s.name === "Todo") ?? states[0];
  const team = teams[0];
  const project = projects.find((p) => p.slugId === "symphony") ?? projects[0];

  async function submitIssue(e: React.FormEvent) {
    e.preventDefault();
    if (!title.trim() || !team || !todo || saving) return;
    setSaving(true);
    try {
      await createIssue({
        teamId: team.id,
        title: title.trim(),
        description: description.trim() || null,
        projectId: project?.id ?? null,
        stateId: todo.id,
      });
      setTitle("");
      setDescription("");
      setDraftOpen(false);
      await mutate();
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="h-full flex flex-col">
      <div className="border-b border-border bg-surface/40 px-4 py-3">
        {!draftOpen ? (
          <button
            onClick={() => setDraftOpen(true)}
            className="h-8 rounded-md border border-border bg-elevated px-3 text-[13px] hover:border-accent/60"
          >
            New issue
          </button>
        ) : (
          <form onSubmit={submitIssue} className="grid max-w-3xl grid-cols-[1fr_auto] gap-2">
            <input
              autoFocus
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder="Issue title"
              className="h-8 rounded-md border border-border bg-bg px-3 text-[13px] outline-none focus:border-accent/70"
            />
            <div className="flex gap-2">
              <button
                type="submit"
                disabled={!title.trim() || !team || !todo || saving}
                className="h-8 rounded-md bg-accent px-3 text-[13px] text-white disabled:cursor-not-allowed disabled:opacity-50"
              >
                {saving ? "Saving" : "Create"}
              </button>
              <button
                type="button"
                onClick={() => setDraftOpen(false)}
                className="h-8 rounded-md border border-border px-3 text-[13px] hover:bg-elevated"
              >
                Cancel
              </button>
            </div>
            <textarea
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="Description"
              className="col-span-2 min-h-20 resize-y rounded-md border border-border bg-bg px-3 py-2 text-[13px] outline-none focus:border-accent/70"
            />
          </form>
        )}
      </div>
      <div className="flex flex-1 gap-3 overflow-x-auto p-4">
        {ordered.map((s) => {
          const list = issues.filter((i) => i.state.id === s.id);
          return (
            <div
              key={s.id}
              className="w-[300px] shrink-0 flex flex-col bg-surface rounded-lg border border-border"
            >
              <div className="h-9 flex items-center gap-2 px-3 border-b border-border">
                <span
                  className="w-2.5 h-2.5 rounded-full"
                  style={{ background: s.color ?? "#5e6ad2" }}
                />
                <span className="text-[13px] font-medium">{s.name}</span>
                <span className="ml-auto text-2xs text-muted">{list.length}</span>
              </div>
              <div
                className="p-2 flex flex-col gap-2 overflow-auto"
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
                        <span
                          key={l.id}
                          className="pill"
                          style={{ borderColor: l.color ?? undefined }}
                        >
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
    </div>
  );
}
