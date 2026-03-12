use crate::{AppState, error::Result};
use crate::models::notification::NotificationListQuery;
use crate::services::auth::User;
use axum::{
    extract::{Path, Query, State},
    response::Json,
    routing::{get, post, put, delete},
    Router,
    Extension,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::info;

pub fn router() -> Router {
    Router::new()
        .route("/", get(list_notifications))
        .route("/unread-count", get(get_unread_count))
        .route("/:notification_id", put(mark_as_read).delete(delete_notification))
        .route("/mark-all-read", post(mark_all_as_read))
}

/// 获取通知列表
/// GET /api/docs/notifications
async fn list_notifications(
    Extension(app_state): Extension<Arc<AppState>>,
    Query(query): Query<NotificationListQuery>,
    user: User,
) -> Result<Json<Value>> {
    // 创建通知服务
    let notification_service = crate::services::notification::NotificationService::new(
        app_state.db.clone(),
        app_state.auth_service.clone(),
        app_state.config.clone(),
    );

    let (notifications, total) = notification_service
        .get_user_notifications(&user.id, query)
        .await?;

    Ok(Json(json!({
        "success": true,
        "data": {
            "notifications": notifications,
            "total": total
        },
        "message": "Notifications retrieved successfully"
    })))
}

/// 获取未读通知数量
/// GET /api/docs/notifications/unread-count
async fn get_unread_count(
    Extension(app_state): Extension<Arc<AppState>>,
    user: User,
) -> Result<Json<Value>> {
    let notification_service = crate::services::notification::NotificationService::new(
        app_state.db.clone(),
        app_state.auth_service.clone(),
        app_state.config.clone(),
    );

    let count = notification_service.get_unread_count(&user.id).await?;

    Ok(Json(json!({
        "success": true,
        "data": {
            "count": count
        },
        "message": "Unread count retrieved successfully"
    })))
}

/// 标记通知为已读
/// PUT /api/docs/notifications/:notification_id
async fn mark_as_read(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(notification_id): Path<String>,
    user: User,
) -> Result<Json<Value>> {
    let notification_service = crate::services::notification::NotificationService::new(
        app_state.db.clone(),
        app_state.auth_service.clone(),
        app_state.config.clone(),
    );

    let notification = notification_service
        .mark_as_read(&user.id, &notification_id)
        .await?;

    info!("User {} marked notification {} as read", user.id, notification_id);

    Ok(Json(json!({
        "success": true,
        "data": notification,
        "message": "Notification marked as read"
    })))
}

/// 标记所有通知为已读
/// POST /api/docs/notifications/mark-all-read
async fn mark_all_as_read(
    Extension(app_state): Extension<Arc<AppState>>,
    user: User,
) -> Result<Json<Value>> {
    let notification_service = crate::services::notification::NotificationService::new(
        app_state.db.clone(),
        app_state.auth_service.clone(),
        app_state.config.clone(),
    );

    let count = notification_service.mark_all_as_read(&user.id).await?;

    info!("User {} marked {} notifications as read", user.id, count);

    Ok(Json(json!({
        "success": true,
        "data": {
            "updated_count": count
        },
        "message": "All notifications marked as read"
    })))
}

/// 删除通知
/// DELETE /api/docs/notifications/:notification_id
async fn delete_notification(
    Extension(app_state): Extension<Arc<AppState>>,
    Path(notification_id): Path<String>,
    user: User,
) -> Result<Json<Value>> {
    let notification_service = crate::services::notification::NotificationService::new(
        app_state.db.clone(),
        app_state.auth_service.clone(),
        app_state.config.clone(),
    );

    notification_service
        .delete_notification(&user.id, &notification_id)
        .await?;

    info!("User {} deleted notification {}", user.id, notification_id);

    Ok(Json(json!({
        "success": true,
        "data": null,
        "message": "Notification deleted successfully"
    })))
}