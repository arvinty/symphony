use async_graphql::*;
use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub type AppSchema = Schema<QueryRoot, MutationRoot, EmptySubscription>;

pub fn build_schema(pool: SqlitePool) -> AppSchema {
    Schema::build(QueryRoot, MutationRoot, EmptySubscription)
        .data(pool)
        .finish()
}

#[derive(SimpleObject, Clone)]
pub struct Team {
    pub id: ID,
    pub key: String,
    pub name: String,
    pub color: Option<String>,
    pub icon: Option<String>,
}

#[derive(SimpleObject, Clone)]
pub struct Project {
    pub id: ID,
    pub slug_id: String,
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub icon: Option<String>,
}

#[derive(SimpleObject, Clone)]
pub struct WorkflowState {
    pub id: ID,
    pub name: String,
    #[graphql(name = "type")]
    pub state_type: String,
    pub position: f64,
    pub color: Option<String>,
}

#[derive(SimpleObject, Clone)]
pub struct Label {
    pub id: ID,
    pub name: String,
    pub color: Option<String>,
}

#[derive(SimpleObject, Clone)]
pub struct LabelConnection {
    pub nodes: Vec<Label>,
}

#[derive(SimpleObject, Clone)]
pub struct User {
    pub id: ID,
    pub name: String,
    pub display_name: String,
    pub email: String,
    pub avatar_url: Option<String>,
}

#[derive(SimpleObject, Clone)]
pub struct IssueRelationRef {
    #[graphql(name = "type")]
    pub rel_type: String,
    pub issue: IssueLite,
}

#[derive(SimpleObject, Clone)]
pub struct IssueLite {
    pub id: ID,
    pub identifier: String,
    pub state: WorkflowState,
}

#[derive(SimpleObject, Clone)]
pub struct InverseRelationConnection {
    pub nodes: Vec<IssueRelationRef>,
}

#[derive(SimpleObject, Clone)]
pub struct Attachment {
    pub id: ID,
    pub kind: String,
    pub url: String,
    pub title: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(SimpleObject, Clone)]
pub struct AttachmentConnection {
    pub nodes: Vec<Attachment>,
}

#[derive(SimpleObject, Clone)]
pub struct Issue {
    pub id: ID,
    pub identifier: String,
    pub number: i32,
    pub title: String,
    pub description: Option<String>,
    pub priority: Option<i32>,
    pub url: Option<String>,
    pub branch_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub state: WorkflowState,
    pub assignee: Option<User>,
    pub project: Option<Project>,
    pub team: Team,
    pub labels: LabelConnection,
    pub inverse_relations: InverseRelationConnection,
    pub attachments: AttachmentConnection,
}

#[derive(SimpleObject, Clone)]
pub struct PageInfo {
    pub has_next_page: bool,
    pub end_cursor: Option<String>,
}

#[derive(SimpleObject, Clone)]
pub struct IssueConnection {
    pub page_info: PageInfo,
    pub nodes: Vec<Issue>,
}

#[derive(InputObject)]
pub struct StringFilter {
    pub eq: Option<String>,
    pub r#in: Option<Vec<String>>,
}

#[derive(InputObject)]
pub struct IdFilter {
    pub r#in: Option<Vec<ID>>,
}

#[derive(InputObject)]
pub struct ProjectFilter {
    pub slug_id: Option<StringFilter>,
}
#[derive(InputObject)]
pub struct WorkflowStateFilter {
    pub name: Option<StringFilter>,
}
#[derive(InputObject)]
pub struct IssueFilter {
    pub project: Option<ProjectFilter>,
    pub state: Option<WorkflowStateFilter>,
    pub id: Option<IdFilter>,
}

pub struct QueryRoot;

#[Object]
impl QueryRoot {
    async fn teams(&self, ctx: &Context<'_>) -> Result<Vec<Team>> {
        let pool = ctx.data::<SqlitePool>()?;
        let rows = sqlx::query("SELECT id, key, name, color, icon FROM teams ORDER BY name")
            .fetch_all(pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| Team {
                id: r.get::<String, _>("id").into(),
                key: r.get("key"),
                name: r.get("name"),
                color: r.try_get("color").ok(),
                icon: r.try_get("icon").ok(),
            })
            .collect())
    }

    async fn projects(&self, ctx: &Context<'_>) -> Result<Vec<Project>> {
        let pool = ctx.data::<SqlitePool>()?;
        let rows = sqlx::query("SELECT id, slug_id, name, description, color, icon FROM projects ORDER BY name")
            .fetch_all(pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| Project {
                id: r.get::<String, _>("id").into(),
                slug_id: r.get("slug_id"),
                name: r.get("name"),
                description: r.try_get("description").ok(),
                color: r.try_get("color").ok(),
                icon: r.try_get("icon").ok(),
            })
            .collect())
    }

    async fn workflow_states(&self, ctx: &Context<'_>) -> Result<Vec<WorkflowState>> {
        let pool = ctx.data::<SqlitePool>()?;
        let rows = sqlx::query("SELECT id, name, type, position, color FROM workflow_states ORDER BY position")
            .fetch_all(pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| WorkflowState {
                id: r.get::<String, _>("id").into(),
                name: r.get("name"),
                state_type: r.get("type"),
                position: r.get::<f64, _>("position"),
                color: r.try_get("color").ok(),
            })
            .collect())
    }

    async fn issue(&self, ctx: &Context<'_>, id: ID) -> Result<Option<Issue>> {
        let pool = ctx.data::<SqlitePool>()?;
        let id_str = id.as_str();
        let row = sqlx::query(
            "SELECT i.id, i.identifier, i.number, i.title, i.description, i.priority, i.url,
                    i.branch_name, i.created_at, i.updated_at, i.assignee_id, i.project_id, i.team_id,
                    ws.id AS state_id, ws.name AS state_name, ws.type AS state_type, ws.position AS state_pos, ws.color AS state_color
             FROM issues i
             JOIN workflow_states ws ON ws.id = i.state_id
             WHERE i.id = ?",
        )
        .bind(id_str)
        .fetch_optional(pool)
        .await?;
        match row {
            None => Ok(None),
            Some(r) => {
                let iid: String = r.get("id");
                let labels = load_labels(pool, &iid).await.unwrap_or_default();
                let inverse = load_inverse(pool, &iid).await.unwrap_or_default();
                let attachments = load_attachments(pool, &iid).await.unwrap_or_default();
                let assignee = match r.try_get::<String, _>("assignee_id").ok() {
                    Some(uid) => load_user(pool, &uid).await.ok(),
                    None => None,
                };
                let project = match r.try_get::<String, _>("project_id").ok() {
                    Some(pid) => load_project(pool, &pid).await.ok(),
                    None => None,
                };
                let team = load_team(pool, &r.get::<String, _>("team_id")).await?;
                Ok(Some(Issue {
                    id: iid.clone().into(),
                    identifier: r.get("identifier"),
                    number: r.get::<i64, _>("number") as i32,
                    title: r.get("title"),
                    description: r.try_get("description").ok(),
                    priority: r.try_get::<i64, _>("priority").ok().map(|v| v as i32),
                    url: r.try_get("url").ok(),
                    branch_name: r.try_get("branch_name").ok(),
                    created_at: parse_dt(r.get::<String, _>("created_at")),
                    updated_at: parse_dt(r.get::<String, _>("updated_at")),
                    state: WorkflowState {
                        id: r.get::<String, _>("state_id").into(),
                        name: r.get("state_name"),
                        state_type: r.get("state_type"),
                        position: r.get::<f64, _>("state_pos"),
                        color: r.try_get("state_color").ok(),
                    },
                    assignee,
                    project,
                    team,
                    labels: LabelConnection { nodes: labels },
                    inverse_relations: InverseRelationConnection { nodes: inverse },
                    attachments: AttachmentConnection { nodes: attachments },
                }))
            }
        }
    }

    async fn issues(
        &self,
        ctx: &Context<'_>,
        first: Option<i32>,
        after: Option<String>,
        filter: Option<IssueFilter>,
    ) -> Result<IssueConnection> {
        let pool = ctx.data::<SqlitePool>()?;
        let limit = first.unwrap_or(50).clamp(1, 250) as i64;
        let offset: i64 = after.as_deref().and_then(|s| s.parse().ok()).unwrap_or(0);

        let mut where_clauses: Vec<String> = vec![];
        let mut binds: Vec<String> = vec![];

        if let Some(f) = &filter {
            if let Some(p) = &f.project {
                if let Some(sf) = &p.slug_id {
                    if let Some(eq) = &sf.eq {
                        where_clauses.push("p.slug_id = ?".into());
                        binds.push(eq.clone());
                    }
                }
            }
            if let Some(s) = &f.state {
                if let Some(sf) = &s.name {
                    if let Some(values) = &sf.r#in {
                        if !values.is_empty() {
                            let placeholders = values.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                            where_clauses.push(format!("ws.name IN ({})", placeholders));
                            for v in values {
                                binds.push(v.clone());
                            }
                        }
                    }
                }
            }
            if let Some(idf) = &f.id {
                if let Some(values) = &idf.r#in {
                    if !values.is_empty() {
                        let placeholders = values.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                        where_clauses.push(format!("i.id IN ({})", placeholders));
                        for v in values {
                            binds.push(v.to_string());
                        }
                    }
                }
            }
        }

        let where_sql = if where_clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_clauses.join(" AND "))
        };
        let sql = format!(
            "SELECT i.id, i.identifier, i.number, i.title, i.description, i.priority, i.url,
                    i.branch_name, i.created_at, i.updated_at, i.assignee_id, i.project_id, i.team_id,
                    ws.id AS state_id, ws.name AS state_name, ws.type AS state_type, ws.position AS state_pos, ws.color AS state_color
             FROM issues i
             JOIN workflow_states ws ON ws.id = i.state_id
             LEFT JOIN projects p ON p.id = i.project_id
             {where_sql}
             ORDER BY i.priority IS NULL, i.priority, i.created_at
             LIMIT ? OFFSET ?",
            where_sql = where_sql
        );
        let mut q = sqlx::query(&sql);
        for b in &binds {
            q = q.bind(b);
        }
        q = q.bind(limit + 1).bind(offset);
        let rows = q.fetch_all(pool).await?;

        let has_next = rows.len() as i64 > limit;
        let mut nodes_raw = rows;
        if has_next {
            nodes_raw.pop();
        }

        let mut nodes = Vec::with_capacity(nodes_raw.len());
        for r in nodes_raw {
            let id_str: String = r.get("id");
            let labels = load_labels(pool, &id_str).await.unwrap_or_default();
            let inverse = load_inverse(pool, &id_str).await.unwrap_or_default();
            let attachments = load_attachments(pool, &id_str).await.unwrap_or_default();
            let assignee = match r.try_get::<String, _>("assignee_id").ok() {
                Some(uid) => load_user(pool, &uid).await.ok(),
                None => None,
            };
            let project = match r.try_get::<String, _>("project_id").ok() {
                Some(pid) => load_project(pool, &pid).await.ok(),
                None => None,
            };
            let team = load_team(pool, &r.get::<String, _>("team_id")).await?;
            nodes.push(Issue {
                id: id_str.clone().into(),
                identifier: r.get("identifier"),
                number: r.get::<i64, _>("number") as i32,
                title: r.get("title"),
                description: r.try_get("description").ok(),
                priority: r.try_get::<i64, _>("priority").ok().map(|v| v as i32),
                url: r.try_get("url").ok(),
                branch_name: r.try_get("branch_name").ok(),
                created_at: parse_dt(r.get::<String, _>("created_at")),
                updated_at: parse_dt(r.get::<String, _>("updated_at")),
                state: WorkflowState {
                    id: r.get::<String, _>("state_id").into(),
                    name: r.get("state_name"),
                    state_type: r.get("state_type"),
                    position: r.get::<f64, _>("state_pos"),
                    color: r.try_get("state_color").ok(),
                },
                assignee,
                project,
                team,
                labels: LabelConnection { nodes: labels },
                inverse_relations: InverseRelationConnection { nodes: inverse },
                attachments: AttachmentConnection { nodes: attachments },
            });
        }
        let end_cursor = if has_next { Some((offset + limit).to_string()) } else { None };
        Ok(IssueConnection {
            page_info: PageInfo {
                has_next_page: has_next,
                end_cursor,
            },
            nodes,
        })
    }
}

pub struct MutationRoot;

#[Object]
impl MutationRoot {
    async fn create_issue(
        &self,
        ctx: &Context<'_>,
        input: CreateIssueInput,
    ) -> Result<String> {
        let pool = ctx.data::<SqlitePool>()?;
        let mut tx = pool.begin().await?;
        let team_id = input.team_id.to_string();
        let team_key: String = sqlx::query_scalar("SELECT key FROM teams WHERE id = ?")
            .bind(&team_id).fetch_one(&mut *tx).await?;
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM issues WHERE team_id = ?")
            .bind(&team_id).fetch_one(&mut *tx).await?;
        let number = count + 1;
        let identifier = format!("{}-{}", team_key, number);
        let id = format!("iss_{}", Uuid::new_v4());
        let state_id: String = match input.state_id {
            Some(s) => s.to_string(),
            None => sqlx::query_scalar("SELECT id FROM workflow_states WHERE team_id = ? AND name = 'Todo' LIMIT 1")
                .bind(&team_id).fetch_one(&mut *tx).await?,
        };
        sqlx::query(
            "INSERT INTO issues (id, identifier, number, title, description, priority, team_id, project_id, state_id, assignee_id) VALUES (?,?,?,?,?,?,?,?,?,?)"
        )
        .bind(&id).bind(&identifier).bind(number).bind(&input.title).bind(&input.description)
        .bind(input.priority).bind(&team_id).bind(input.project_id.map(|i| i.to_string()))
        .bind(&state_id).bind(input.assignee_id.map(|i| i.to_string()))
        .execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(identifier)
    }

    async fn update_issue_state(&self, ctx: &Context<'_>, id: ID, state_id: ID) -> Result<bool> {
        let pool = ctx.data::<SqlitePool>()?;
        sqlx::query("UPDATE issues SET state_id = ?, updated_at = datetime('now') WHERE id = ?")
            .bind(state_id.to_string())
            .bind(id.to_string())
            .execute(pool)
            .await?;
        Ok(true)
    }

    async fn add_attachment(
        &self,
        ctx: &Context<'_>,
        issue_id: ID,
        url: String,
        title: Option<String>,
    ) -> Result<Attachment> {
        let pool = ctx.data_unchecked::<SqlitePool>();
        let id = format!("att_{}", Uuid::new_v4());
        let now = Utc::now();
        sqlx::query("INSERT INTO attachments(id, issue_id, kind, url, title, created_at) VALUES (?,?,?,?,?,?)")
            .bind(&id).bind(issue_id.as_str()).bind("pull_request")
            .bind(&url).bind(&title).bind(now.to_rfc3339())
            .execute(pool).await?;
        Ok(Attachment { id: id.into(), kind: "pull_request".into(), url, title, created_at: now })
    }

    async fn remove_attachment(&self, ctx: &Context<'_>, id: ID) -> Result<bool> {
        let pool = ctx.data_unchecked::<SqlitePool>();
        let n = sqlx::query("DELETE FROM attachments WHERE id = ?")
            .bind(id.as_str()).execute(pool).await?.rows_affected();
        Ok(n > 0)
    }
}

#[derive(InputObject)]
pub struct CreateIssueInput {
    pub team_id: ID,
    pub title: String,
    pub description: Option<String>,
    pub priority: Option<i32>,
    pub project_id: Option<ID>,
    pub state_id: Option<ID>,
    pub assignee_id: Option<ID>,
}

async fn load_attachments(pool: &SqlitePool, issue_id: &str) -> Result<Vec<Attachment>> {
    let rows = sqlx::query(
        "SELECT id, kind, url, title, created_at FROM attachments WHERE issue_id = ? ORDER BY created_at ASC",
    )
    .bind(issue_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| Attachment {
            id: r.get::<String, _>("id").into(),
            kind: r.get("kind"),
            url: r.get("url"),
            title: r.try_get("title").ok(),
            created_at: {
                let s: String = r.get("created_at");
                chrono::DateTime::parse_from_rfc3339(&s)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now())
            },
        })
        .collect())
}

async fn load_labels(pool: &SqlitePool, issue_id: &str) -> Result<Vec<Label>> {
    let rows = sqlx::query(
        "SELECT l.id, l.name, l.color FROM labels l
         JOIN issue_labels il ON il.label_id = l.id
         WHERE il.issue_id = ?",
    )
    .bind(issue_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| Label {
            id: r.get::<String, _>("id").into(),
            name: r.get("name"),
            color: r.try_get("color").ok(),
        })
        .collect())
}

async fn load_inverse(pool: &SqlitePool, issue_id: &str) -> Result<Vec<IssueRelationRef>> {
    let rows = sqlx::query(
        "SELECT r.type, i.id AS rid, i.identifier AS ridentifier,
                ws.id AS sid, ws.name AS sname, ws.type AS stype, ws.position AS spos, ws.color AS scolor
         FROM issue_relations r
         JOIN issues i ON i.id = r.related_issue_id
         JOIN workflow_states ws ON ws.id = i.state_id
         WHERE r.issue_id = ?",
    )
    .bind(issue_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| IssueRelationRef {
            rel_type: r.get("type"),
            issue: IssueLite {
                id: r.get::<String, _>("rid").into(),
                identifier: r.get("ridentifier"),
                state: WorkflowState {
                    id: r.get::<String, _>("sid").into(),
                    name: r.get("sname"),
                    state_type: r.get("stype"),
                    position: r.get::<f64, _>("spos"),
                    color: r.try_get("scolor").ok(),
                },
            },
        })
        .collect())
}

async fn load_user(pool: &SqlitePool, id: &str) -> Result<User> {
    let r = sqlx::query("SELECT id, name, display_name, email, avatar_url FROM users WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await?;
    Ok(User {
        id: r.get::<String, _>("id").into(),
        name: r.get("name"),
        display_name: r.get("display_name"),
        email: r.get("email"),
        avatar_url: r.try_get("avatar_url").ok(),
    })
}

async fn load_project(pool: &SqlitePool, id: &str) -> Result<Project> {
    let r = sqlx::query("SELECT id, slug_id, name, description, color, icon FROM projects WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await?;
    Ok(Project {
        id: r.get::<String, _>("id").into(),
        slug_id: r.get("slug_id"),
        name: r.get("name"),
        description: r.try_get("description").ok(),
        color: r.try_get("color").ok(),
        icon: r.try_get("icon").ok(),
    })
}

async fn load_team(pool: &SqlitePool, id: &str) -> Result<Team> {
    let r = sqlx::query("SELECT id, key, name, color, icon FROM teams WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await?;
    Ok(Team {
        id: r.get::<String, _>("id").into(),
        key: r.get("key"),
        name: r.get("name"),
        color: r.try_get("color").ok(),
        icon: r.try_get("icon").ok(),
    })
}

fn parse_dt(s: String) -> DateTime<Utc> {
    // SQLite "YYYY-MM-DD HH:MM:SS" format -> RFC3339
    let s = s.replace(' ', "T") + "Z";
    chrono::DateTime::parse_from_rfc3339(&s)
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}
