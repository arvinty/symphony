CREATE TABLE IF NOT EXISTS attachments (
    id TEXT PRIMARY KEY,
    issue_id TEXT NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    url TEXT NOT NULL,
    title TEXT,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_attachments_issue ON attachments(issue_id);
