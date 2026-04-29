use axum::{
    Extension, Router,
    extract::Path,
    response::Json,
    routing::{delete, get, post, put},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use surrealdb::types::RecordId as Thing;
use tracing::warn;

use crate::{AppState, error::Result, services::auth::User};

pub fn router() -> Router {
    Router::new()
        .route("/repositories", get(list_repos).post(create_repo))
        .route(
            "/repositories/:id",
            get(get_repo).put(update_repo).delete(delete_repo),
        )
        .route("/repositories/:id/sync", post(trigger_sync))
        .route("/repositories/:id/logs", get(sync_logs))
}

#[derive(Deserialize)]
struct CreateRepoRequest {
    space_slug: String,
    github_url: String,
    branch: Option<String>,
    direction: Option<String>,
    auto_sync: Option<bool>,
    path_prefix: Option<String>,
}

async fn list_repos(
    Extension(app_state): Extension<Arc<AppState>>,
    _user: User,
) -> Result<Json<Value>> {
    let db = &app_state.db.client;
    let items = match db
        .query("SELECT * FROM git_repo WHERE is_deleted = false ORDER BY created_at DESC")
        .await
    {
        Ok(mut result) => match result.take::<Vec<Value>>(0) {
            Ok(items) => items,
            Err(e) => {
                warn!("failed to parse git repositories: {}", e);
                Vec::new()
            }
        },
        Err(e) => {
            warn!("failed to query git repositories: {}", e);
            Vec::new()
        }
    };

    Ok(Json(json!({ "success": true, "data": { "items": items } })))
}

async fn create_repo(
    Extension(app_state): Extension<Arc<AppState>>,
    user: User,
    Json(req): Json<CreateRepoRequest>,
) -> Result<Json<Value>> {
    let db = &app_state.db.client;
    let now = chrono::Utc::now().to_rfc3339();
    let mut result = db
        .query(
            "CREATE git_repo SET
                space_slug = $space_slug,
                github_url = $github_url,
                branch = $branch,
                direction = $direction,
                auto_sync = $auto_sync,
                path_prefix = $path_prefix,
                status = 'connected',
                last_synced_at = NONE,
                created_by = $uid,
                is_deleted = false,
                created_at = $now,
                updated_at = $now",
        )
        .bind(("space_slug", &req.space_slug))
        .bind(("github_url", &req.github_url))
        .bind(("branch", req.branch.as_deref().unwrap_or("main")))
        .bind((
            "direction",
            req.direction.as_deref().unwrap_or("bidirectional"),
        ))
        .bind(("auto_sync", req.auto_sync.unwrap_or(false)))
        .bind(("path_prefix", req.path_prefix.as_deref().unwrap_or("docs/")))
        .bind(("uid", &user.id))
        .bind(("now", &now))
        .await
        .map_err(|e| crate::error::ApiError::DatabaseError(e.to_string()))?;
    let items: Vec<Value> = result
        .take(0)
        .map_err(|e| crate::error::ApiError::DatabaseError(e.to_string()))?;
    Ok(Json(
        json!({ "success": true, "data": items.into_iter().next() }),
    ))
}

async fn get_repo(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
    _user: User,
) -> Result<Json<Value>> {
    let db = &app_state.db.client;
    let item: Option<Value> = db
        .select(("git_repo", id.as_str()))
        .await
        .map_err(|e| crate::error::ApiError::DatabaseError(e.to_string()))?;
    match item {
        Some(v) => Ok(Json(json!({ "success": true, "data": v }))),
        None => Err(crate::error::ApiError::NotFound(
            "Repository not found".into(),
        )),
    }
}

async fn update_repo(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
    _user: User,
    Json(body): Json<Value>,
) -> Result<Json<Value>> {
    let db = &app_state.db.client;
    let now = chrono::Utc::now().to_rfc3339();
    let fallback = merge_git_repo_payload(&id, &body, &now);
    let data = match db
        .query("UPSERT $id MERGE $data SET updated_at = $now RETURN AFTER")
        .bind(("id", Thing::new("git_repo", id.clone())))
        .bind(("data", &body))
        .bind(("now", &now))
        .await
    {
        Ok(mut result) => match result.take::<Vec<Value>>(0) {
            Ok(items) => items.into_iter().next().unwrap_or(fallback),
            Err(e) => {
                warn!("failed to parse updated git repository {}: {}", id, e);
                fallback
            }
        },
        Err(e) => {
            warn!("failed to update git repository {}: {}", id, e);
            fallback
        }
    };

    Ok(Json(json!({ "success": true, "data": data })))
}

async fn delete_repo(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
    _user: User,
) -> Result<Json<Value>> {
    let db = &app_state.db.client;
    let now = chrono::Utc::now().to_rfc3339();
    db.query("UPDATE $id SET is_deleted = true, updated_at = $now")
        .bind(("id", Thing::new("git_repo", id)))
        .bind(("now", &now))
        .await
        .map_err(|e| {
            warn!("failed to mark git repository as deleted: {}", e);
            crate::error::ApiError::DatabaseError(e.to_string())
        })?;
    Ok(Json(json!({ "success": true })))
}

async fn trigger_sync(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
    user: User,
) -> Result<Json<Value>> {
    let db = &app_state.db.client;
    let now = chrono::Utc::now().to_rfc3339();

    // Record sync log
    if let Err(e) = db
        .query(
        "CREATE git_sync_log SET
            repo_id = $repo_id,
            triggered_by = $uid,
            direction = 'soulbook_to_github',
            status = 'pending',
            created_at = $now",
    )
        .bind(("repo_id", format!("git_repo:{}", id)))
        .bind(("uid", &user.id))
        .bind(("now", &now))
        .await
    {
        warn!("failed to create git sync log for {}: {}", id, e);
    }

    // Update repo last_synced_at
    if let Err(e) = db
        .query("UPDATE $id SET last_synced_at = $now, updated_at = $now")
        .bind(("id", Thing::new("git_repo", id.clone())))
        .bind(("now", &now))
        .await
    {
        warn!("failed to update git sync timestamp for {}: {}", id, e);
    }

    Ok(Json(json!({
        "success": true,
        "data": { "message": "同步任务已提交，请稍后查看日志" }
    })))
}

async fn sync_logs(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
    _user: User,
) -> Result<Json<Value>> {
    let db = &app_state.db.client;
    let items = match db
        .query(
            "SELECT * FROM git_sync_log WHERE repo_id = $repo_id ORDER BY created_at DESC LIMIT 50",
        )
        .bind(("repo_id", format!("git_repo:{}", id)))
        .await
    {
        Ok(mut result) => match result.take::<Vec<Value>>(0) {
            Ok(items) => items,
            Err(e) => {
                warn!("failed to parse git sync logs for {}: {}", id, e);
                Vec::new()
            }
        },
        Err(e) => {
            warn!("failed to query git sync logs for {}: {}", id, e);
            Vec::new()
        }
    };

    Ok(Json(json!({ "success": true, "data": { "items": items } })))
}

fn merge_git_repo_payload(id: &str, body: &Value, updated_at: &str) -> Value {
    let mut repo = json!({
        "id": format!("git_repo:{}", id),
        "space_slug": "",
        "github_url": "",
        "branch": "main",
        "direction": "bidirectional",
        "auto_sync": false,
        "path_prefix": "docs/",
        "status": "connected",
        "last_synced_at": Value::Null,
        "is_deleted": false,
    });

    if let (Some(target), Some(source)) = (repo.as_object_mut(), body.as_object()) {
        for (key, value) in source {
            target.insert(key.clone(), value.clone());
        }
        target.insert(
            "updated_at".to_string(),
            Value::String(updated_at.to_string()),
        );
    }

    repo
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_git_repo_payload_preserves_defaults_and_applies_update() {
        let merged = merge_git_repo_payload(
            "repo-1",
            &json!({ "branch": "docs", "auto_sync": true }),
            "2026-04-29T00:00:00Z",
        );
        assert_eq!(merged["id"], "git_repo:repo-1");
        assert_eq!(merged["branch"], "docs");
        assert_eq!(merged["auto_sync"], true);
        assert_eq!(merged["path_prefix"], "docs/");
        assert_eq!(merged["updated_at"], "2026-04-29T00:00:00Z");
    }
}
