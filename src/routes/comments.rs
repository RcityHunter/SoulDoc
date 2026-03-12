use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post, put, delete},
    Extension,
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{
    error::ApiError,
    models::comment::{Comment, CreateCommentRequest, UpdateCommentRequest},
    services::{auth::AuthService, comments::CommentService},
};

#[derive(Deserialize)]
pub struct CommentQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
    pub sort: Option<String>,
}

#[derive(Serialize)]
pub struct CommentListResponse {
    pub comments: Vec<Comment>,
    pub total_count: i64,
    pub page: i64,
    pub per_page: i64,
    pub total_pages: i64,
}

pub async fn get_document_comments(
    Path(document_id): Path<String>,
    Query(query): Query<CommentQuery>,
    Extension(app_state): Extension<Arc<crate::AppState>>,
    Extension(user_id): Extension<String>,
) -> Result<Json<CommentListResponse>, ApiError> {
    let comment_service = &app_state.comment_service;
    let auth_service = &app_state.auth_service;
    auth_service
        .check_permission(&user_id, "docs.comment.read", Some(&document_id))
        .await?;

    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(20);

    let comments = comment_service
        .get_document_comments(&document_id, page, per_page)
        .await?;

    let total_count = comment_service
        .get_document_comments_count(&document_id)
        .await?;

    let total_pages = (total_count + per_page - 1) / per_page;

    Ok(Json(CommentListResponse {
        comments,
        total_count,
        page,
        per_page,
        total_pages,
    }))
}

pub async fn create_comment(
    Path(document_id): Path<String>,
    Extension(app_state): Extension<Arc<crate::AppState>>,
    Extension(user_id): Extension<String>,
    Json(request): Json<CreateCommentRequest>,
) -> Result<Json<Comment>, ApiError> {
    let comment_service = &app_state.comment_service;
    let auth_service = &app_state.auth_service;
    auth_service
        .check_permission(&user_id, "docs.comment.create", Some(&document_id))
        .await?;

    let comment = comment_service
        .create_comment(&document_id, &user_id, request)
        .await?;

    Ok(Json(comment))
}

pub async fn get_comment(
    Path(comment_id): Path<String>,
    Extension(app_state): Extension<Arc<crate::AppState>>,
    Extension(user_id): Extension<String>,
) -> Result<Json<Comment>, ApiError> {
    let comment_service = &app_state.comment_service;
    let auth_service = &app_state.auth_service;
    let comment = comment_service.get_comment(&comment_id).await?;
    
    auth_service
        .check_permission(&user_id, "docs.comment.read", Some(&comment.document_id.to_string()))
        .await?;

    Ok(Json(comment))
}

pub async fn update_comment(
    Path(comment_id): Path<String>,
    Extension(app_state): Extension<Arc<crate::AppState>>,
    Extension(user_id): Extension<String>,
    Json(request): Json<UpdateCommentRequest>,
) -> Result<Json<Comment>, ApiError> {
    let comment_service = &app_state.comment_service;
    let auth_service = &app_state.auth_service;
    let comment = comment_service.get_comment(&comment_id).await?;
    
    if comment.author_id != user_id {
        auth_service
            .check_permission(&user_id, "docs.comment.update", Some(&comment.document_id.to_string()))
            .await?;
    }

    let updated_comment = comment_service
        .update_comment(&comment_id, &user_id, request)
        .await?;

    Ok(Json(updated_comment))
}

pub async fn delete_comment(
    Path(comment_id): Path<String>,
    Extension(app_state): Extension<Arc<crate::AppState>>,
    Extension(user_id): Extension<String>,
) -> Result<StatusCode, ApiError> {
    let comment_service = &app_state.comment_service;
    let auth_service = &app_state.auth_service;
    let comment = comment_service.get_comment(&comment_id).await?;
    
    if comment.author_id != user_id {
        auth_service
            .check_permission(&user_id, "docs.comment.delete", Some(&comment.document_id.to_string()))
            .await?;
    }

    comment_service.delete_comment(&comment_id, &user_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_comment_replies(
    Path(comment_id): Path<String>,
    Query(query): Query<CommentQuery>,
    Extension(app_state): Extension<Arc<crate::AppState>>,
    Extension(user_id): Extension<String>,
) -> Result<Json<CommentListResponse>, ApiError> {
    let comment_service = &app_state.comment_service;
    let auth_service = &app_state.auth_service;
    let comment = comment_service.get_comment(&comment_id).await?;
    
    auth_service
        .check_permission(&user_id, "docs.comment.read", Some(&comment.document_id.to_string()))
        .await?;

    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(20);

    let replies = comment_service
        .get_comment_replies(&comment_id, page, per_page)
        .await?;

    let total_count = comment_service
        .get_comment_replies_count(&comment_id)
        .await?;

    let total_pages = (total_count + per_page - 1) / per_page;

    Ok(Json(CommentListResponse {
        comments: replies,
        total_count,
        page,
        per_page,
        total_pages,
    }))
}

pub async fn toggle_comment_like(
    Path(comment_id): Path<String>,
    Extension(app_state): Extension<Arc<crate::AppState>>,
    Extension(user_id): Extension<String>,
) -> Result<Json<Comment>, ApiError> {
    let comment_service = &app_state.comment_service;
    let auth_service = &app_state.auth_service;
    let comment = comment_service.get_comment(&comment_id).await?;
    
    auth_service
        .check_permission(&user_id, "docs.comment.read", Some(&comment.document_id.to_string()))
        .await?;

    let updated_comment = comment_service
        .toggle_comment_like(&comment_id, &user_id)
        .await?;

    Ok(Json(updated_comment))
}

pub fn router() -> Router {
    Router::new()
        .route("/document/:document_id", get(get_document_comments).post(create_comment))
        .route("/:comment_id", get(get_comment).put(update_comment).delete(delete_comment))
        .route("/:comment_id/replies", get(get_comment_replies))
        .route("/:comment_id/like", post(toggle_comment_like))
}