import React from "react";
import ReactDOM from "react-dom/client";
import { BrowserRouter, Routes, Route, Navigate } from "react-router-dom";
import "./index.css";
import { App } from "./App";
import { InboxView } from "./views/InboxView";
import { IssuesView } from "./views/IssuesView";
import { BoardView } from "./views/BoardView";
import { IssueDetailView } from "./views/IssueDetailView";
import { ProjectView } from "./views/ProjectView";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<App />}>
          <Route index element={<Navigate to="/my-issues" replace />} />
          <Route path="inbox" element={<InboxView />} />
          <Route path="my-issues" element={<IssuesView scope="me" />} />
          <Route path="active" element={<IssuesView scope="active" />} />
          <Route path="backlog" element={<IssuesView scope="backlog" />} />
          <Route path="board" element={<BoardView />} />
          <Route path="project/:slug" element={<ProjectView />} />
          <Route path="issue/:identifier" element={<IssueDetailView />} />
        </Route>
      </Routes>
    </BrowserRouter>
  </React.StrictMode>,
);
