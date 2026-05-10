use crate::AppState;
use axum::{extract::State, routing::get, Json, Router};
use serde_json::{json, Value};
use sqlx::Row;

pub fn router() -> Router<AppState> {
    Router::new().route("/issues", get(list_issues))
}

async fn list_issues(State(s): State<AppState>) -> Json<Value> {
    let rows = sqlx::query(
        "SELECT i.id, i.identifier, i.title, i.priority, ws.name AS state
         FROM issues i JOIN workflow_states ws ON ws.id = i.state_id
         ORDER BY i.priority IS NULL, i.priority, i.created_at",
    )
    .fetch_all(&s.pool)
    .await
    .unwrap_or_default();
    let out: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            json!({
                "id": r.get::<String, _>("id"),
                "identifier": r.get::<String, _>("identifier"),
                "title": r.get::<String, _>("title"),
                "priority": r.try_get::<i64, _>("priority").ok(),
                "state": r.get::<String, _>("state"),
            })
        })
        .collect();
    Json(json!({ "issues": out }))
}
