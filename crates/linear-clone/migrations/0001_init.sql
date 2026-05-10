CREATE TABLE IF NOT EXISTS teams (
  id TEXT PRIMARY KEY,
  key TEXT NOT NULL UNIQUE,
  name TEXT NOT NULL,
  color TEXT,
  icon TEXT,
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS projects (
  id TEXT PRIMARY KEY,
  slug_id TEXT NOT NULL UNIQUE,
  name TEXT NOT NULL,
  description TEXT,
  color TEXT,
  icon TEXT,
  team_id TEXT REFERENCES teams(id),
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS workflow_states (
  id TEXT PRIMARY KEY,
  team_id TEXT NOT NULL REFERENCES teams(id),
  name TEXT NOT NULL,
  type TEXT NOT NULL,
  position REAL NOT NULL DEFAULT 0,
  color TEXT
);

CREATE TABLE IF NOT EXISTS labels (
  id TEXT PRIMARY KEY,
  team_id TEXT REFERENCES teams(id),
  name TEXT NOT NULL,
  color TEXT
);

CREATE TABLE IF NOT EXISTS users (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  display_name TEXT NOT NULL,
  email TEXT NOT NULL UNIQUE,
  avatar_url TEXT
);

CREATE TABLE IF NOT EXISTS issues (
  id TEXT PRIMARY KEY,
  identifier TEXT NOT NULL UNIQUE,
  number INTEGER NOT NULL,
  title TEXT NOT NULL,
  description TEXT,
  priority INTEGER,
  team_id TEXT NOT NULL REFERENCES teams(id),
  project_id TEXT REFERENCES projects(id),
  state_id TEXT NOT NULL REFERENCES workflow_states(id),
  assignee_id TEXT REFERENCES users(id),
  branch_name TEXT,
  url TEXT,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_issues_state ON issues(state_id);
CREATE INDEX IF NOT EXISTS idx_issues_project ON issues(project_id);
CREATE INDEX IF NOT EXISTS idx_issues_team ON issues(team_id);

CREATE TABLE IF NOT EXISTS issue_labels (
  issue_id TEXT NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
  label_id TEXT NOT NULL REFERENCES labels(id) ON DELETE CASCADE,
  PRIMARY KEY (issue_id, label_id)
);

CREATE TABLE IF NOT EXISTS issue_relations (
  id TEXT PRIMARY KEY,
  issue_id TEXT NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
  related_issue_id TEXT NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
  type TEXT NOT NULL  -- 'blocks' | 'related' | 'duplicate'
);

CREATE TABLE IF NOT EXISTS comments (
  id TEXT PRIMARY KEY,
  issue_id TEXT NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
  author_id TEXT REFERENCES users(id),
  body TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
