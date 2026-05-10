-- Seed for local dev
INSERT OR IGNORE INTO users (id, name, display_name, email, avatar_url) VALUES
  ('user_arvin', 'arvin', 'Arvin', 'arvintay@gmail.com', NULL),
  ('user_bot',   'symphony', 'Symphony', 'bot@symphony.local', NULL);

INSERT OR IGNORE INTO teams (id, key, name, color, icon) VALUES
  ('team_eng', 'ENG', 'Engineering', '#5e6ad2', 'cube');

INSERT OR IGNORE INTO projects (id, slug_id, name, description, color, team_id) VALUES
  ('proj_symphony', 'symphony', 'Symphony', 'Agent orchestrator', '#a8a8ff', 'team_eng');

INSERT OR IGNORE INTO workflow_states (id, team_id, name, type, position, color) VALUES
  ('s_backlog',  'team_eng', 'Backlog',     'backlog',   0, '#bec2c8'),
  ('s_todo',     'team_eng', 'Todo',        'unstarted', 1, '#e2e2e2'),
  ('s_inprog',   'team_eng', 'In Progress', 'started',   2, '#f2c94c'),
  ('s_review',   'team_eng', 'In Review',   'started',   3, '#5e6ad2'),
  ('s_done',     'team_eng', 'Done',        'completed', 4, '#5fb286'),
  ('s_cancel',   'team_eng', 'Cancelled',   'cancelled', 5, '#95a2b3');

INSERT OR IGNORE INTO labels (id, team_id, name, color) VALUES
  ('l_bug',     'team_eng', 'Bug',     '#eb5757'),
  ('l_feature', 'team_eng', 'Feature', '#5e6ad2'),
  ('l_chore',   'team_eng', 'Chore',   '#bec2c8');

INSERT OR IGNORE INTO issues (id, identifier, number, title, description, priority, team_id, project_id, state_id, assignee_id, branch_name) VALUES
  ('iss_1', 'ENG-1', 1, 'Wire Symphony to Linear-clone', 'Replace file-mock with our local GraphQL.', 1, 'team_eng', 'proj_symphony', 's_todo', 'user_arvin', 'arvin/eng-1-wire-symphony'),
  ('iss_2', 'ENG-2', 2, 'Pixel-polish the issue list',     'Match Linear''s row spacing and chevrons.', 2, 'team_eng', 'proj_symphony', 's_inprog', 'user_arvin', NULL),
  ('iss_3', 'ENG-3', 3, 'Add Cmd+K command bar',           'Quick-jump and actions.',                  2, 'team_eng', 'proj_symphony', 's_backlog', NULL, NULL),
  ('iss_4', 'ENG-4', 4, 'Hermes harness end-to-end',       'Drive a real run via Hermes.',             3, 'team_eng', 'proj_symphony', 's_todo', NULL, NULL);

INSERT OR IGNORE INTO issue_labels (issue_id, label_id) VALUES
  ('iss_1', 'l_feature'),
  ('iss_2', 'l_chore'),
  ('iss_3', 'l_feature'),
  ('iss_4', 'l_feature');
