import { NavLink } from "react-router-dom";
import useSWR from "swr";
import { listProjects } from "../graphql";

const item =
  "flex items-center gap-2 px-2 h-7 rounded text-[13px] text-subtle hover:bg-elevated hover:text-text";
const active = "bg-elevated text-text";

export function Sidebar() {
  const { data: projects = [] } = useSWR("projects", listProjects);
  return (
    <aside className="w-[244px] shrink-0 bg-surface border-r border-border flex flex-col">
      <div className="h-12 flex items-center px-3 gap-2 border-b border-border">
        <div className="w-6 h-6 rounded bg-accent/80 flex items-center justify-center text-[11px] font-semibold">
          A
        </div>
        <div className="flex-1 truncate text-[13px] font-medium">Arvin's Workspace</div>
        <button className="icon-btn">⌄</button>
      </div>
      <div className="px-2 py-3 space-y-0.5">
        <NavLink to="/inbox" className={({ isActive }) => `${item} ${isActive ? active : ""}`}>
          <Icon name="inbox" /> Inbox
          <span className="ml-auto text-2xs text-muted">3</span>
        </NavLink>
        <NavLink to="/my-issues" className={({ isActive }) => `${item} ${isActive ? active : ""}`}>
          <Icon name="me" /> My issues
        </NavLink>
      </div>

      <SidebarSection title="Workspace">
        <NavLink to="/active" className={({ isActive }) => `${item} ${isActive ? active : ""}`}>
          <Icon name="play" /> Active
        </NavLink>
        <NavLink to="/backlog" className={({ isActive }) => `${item} ${isActive ? active : ""}`}>
          <Icon name="dot" /> Backlog
        </NavLink>
        <NavLink to="/board" className={({ isActive }) => `${item} ${isActive ? active : ""}`}>
          <Icon name="board" /> Board
        </NavLink>
      </SidebarSection>

      <SidebarSection title="Projects">
        {projects.map((p: any) => (
          <NavLink
            key={p.id}
            to={`/project/${p.slugId}`}
            className={({ isActive }) => `${item} ${isActive ? active : ""}`}
          >
            <span
              className="w-2.5 h-2.5 rounded-sm"
              style={{ background: p.color ?? "#5e6ad2" }}
            />
            {p.name}
          </NavLink>
        ))}
      </SidebarSection>

      <div className="mt-auto px-3 py-2 border-t border-border text-2xs text-muted">
        v0.1 · Linear-clone
      </div>
    </aside>
  );
}

function SidebarSection({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="px-2 mt-3">
      <div className="text-2xs uppercase text-muted px-2 mb-1 tracking-wide">{title}</div>
      <div className="space-y-0.5">{children}</div>
    </div>
  );
}

function Icon({ name }: { name: string }) {
  const map: Record<string, string> = {
    inbox: "▤",
    me: "◉",
    play: "▶",
    dot: "○",
    board: "▦",
  };
  return <span className="w-4 inline-block text-center text-muted">{map[name] ?? "•"}</span>;
}
