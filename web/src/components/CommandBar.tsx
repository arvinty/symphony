import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import useSWR from "swr";
import { listIssues } from "../graphql";

export function CommandBar({ onClose }: { onClose: () => void }) {
  const [q, setQ] = useState("");
  const nav = useNavigate();
  const { data: issues = [] } = useSWR("issues_all_for_cmd", () => listIssues());

  const filtered = issues
    .filter((i) =>
      [i.identifier, i.title].some((t) => t.toLowerCase().includes(q.toLowerCase())),
    )
    .slice(0, 8);

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-[15vh] bg-black/40"
      onClick={onClose}
    >
      <div
        className="w-[640px] rounded-lg bg-elevated border border-border shadow-popover overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        <input
          autoFocus
          value={q}
          onChange={(e) => setQ(e.target.value)}
          placeholder="Type a command or search…"
          className="w-full bg-transparent border-0 outline-none px-4 h-12 text-[14px] placeholder:text-muted"
        />
        <div className="border-t border-border max-h-80 overflow-auto">
          {filtered.length === 0 && (
            <div className="px-4 py-6 text-subtle text-[13px]">No results.</div>
          )}
          {filtered.map((i) => (
            <button
              key={i.id}
              onClick={() => {
                nav(`/issue/${i.identifier}`);
                onClose();
              }}
              className="w-full text-left px-4 h-9 flex items-center gap-3 hover:bg-bg/40"
            >
              <span className="text-2xs text-muted w-16">{i.identifier}</span>
              <span className="truncate text-[13px]">{i.title}</span>
              <span className="ml-auto pill">{i.state.name}</span>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
