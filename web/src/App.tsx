import { Outlet } from "react-router-dom";
import { Sidebar } from "./components/Sidebar";
import { TopBar } from "./components/TopBar";
import { CommandBar } from "./components/CommandBar";
import { useEffect, useState } from "react";

export function App() {
  const [cmdOpen, setCmdOpen] = useState(false);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setCmdOpen((v) => !v);
      }
      if (e.key === "Escape") setCmdOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className="flex h-full w-full bg-bg text-text">
      <Sidebar />
      <div className="flex-1 flex flex-col min-w-0">
        <TopBar onOpenCmd={() => setCmdOpen(true)} />
        <main className="flex-1 overflow-auto">
          <Outlet />
        </main>
      </div>
      {cmdOpen && <CommandBar onClose={() => setCmdOpen(false)} />}
    </div>
  );
}
