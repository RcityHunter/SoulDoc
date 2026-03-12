use std::sync::Arc;
use surrealdb::types::RecordId as Thing;
use validator::Validate;

use crate::{
    error::ApiError,
    models::comment::{Comment, CreateCommentRequest, UpdateCommentRequest},
    services::{auth::AuthService, database::{Database, record_id_to_string}},
};

#[derive(Clone)]
pub struct CommentService {
    db: Arc<Database>,
    auth_service: Arc<AuthService>,
}

impl CommentService {
    pub fn new(db: Arc<Database>, auth_service: Arc<AuthService>) -> Self {
        Self { db, auth_service }
    }

    pub async fn create_comment(
        &self,
        document_id: &str,
        author_id: &str,
        request: CreateCommentRequest,
    ) -> Result<Comment, ApiError> {
        request.validate()?;

        let document_thing = Thing::new("document", document_id);
        let parent_id = if let Some(parent_id_str) = &request.parent_id {
            Some(Thing::new("comment", parent_id_str.as_str()))
        } else {
            None
        };

        let mut comment = Comment::new(
            record_id_to_string(&document_thing),
            author_id.to_string(),
            request.content,
        );

        if let Some(parent_id) = parent_id {
            comment = comment.with_parent(record_id_to_string(&parent_id));
        }

        let created: Vec<Comment> = self.db.client
            .create("comment")
            .content(comment)
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        created
            .into_iter()
            .next()
            .ok_or_else(|| ApiError::InternalServerError("Failed to create comment".to_string()))
    }

    pub async fn get_comment(&self, comment_id: &str) -> Result<Comment, ApiError> {
        let comment: Option<Comment> = self.db.client
            .select(("comment", comment_id))
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        comment.ok_or_else(|| ApiError::NotFound("Comment not found".to_string()))
    }

    pub async fn update_comment(
        &self,
        comment_id: &str,
        editor_id: &str,
        request: UpdateCommentRequest,
    ) -> Result<Comment, ApiError> {
        request.validate()?;

        let mut comment = self.get_comment(comment_id).await?;
        
        if let Some(content) = request.content {
            comment.update_content(content, editor_id.to_string());
        }

        let updated: Option<Comment> = self.db.client
            .update(("comment", comment_id))
            .content(comment)
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        updated.ok_or_else(|| ApiError::InternalServerError("Failed to update comment".to_string()))
    }

    pub async fn delete_comment(&self, comment_id: &str, deleter_id: &str) -> Result<(), ApiError> {
        let mut comment = self.get_comment(comment_id).await?;
        comment.soft_delete(deleter_id.to_string());

        let _: Option<Comment> = self.db.client
            .update(("comment", comment_id))
            .content(comment)
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    pub async fn get_document_comments(
        &self,
        document_id: &str,
        page: i64,
        per_page: i64,
    ) -> Result<Vec<Comment>, ApiError> {
        let offset = (page - 1) * per_page;
        
        let query = "
            SELECT * FROM comment 
            WHERE document_id = $document_id 
            AND parent_id IS NULL 
            AND is_deleted = false
            ORDER BY created_at DESC
            LIMIT $limit START $offset
        ";

        let comments: Vec<Comment> = self.db.client
            .query(query)
            .bind(("document_id", Thing::new("document", document_id)))
            .bind(("limit", per_page))
            .bind(("offset", offset))
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?
            .take(0)
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        Ok(comments)
    }

    pub async fn get_document_comments_count(&self, document_id: &str) -> Result<i64, ApiError> {
        let query = "
            SELECT count() FROM comment 
            WHERE document_id = $document_id 
            AND parent_id IS NULL 
            AND is_deleted = false
            GROUP ALL
        ";

        let result: Vec<serde_json::Value> = self.db.client
            .query(query)
            .bind(("document_id", Thing::new("document", document_id)))
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?
            .take(0)
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        let count = result
            .first()
            .and_then(|v| v.get("count"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        Ok(count)
    }

    pub async fn get_comment_replies(
        &self,
        parent_id: &str,
        page: i64,
        per_page: i64,
    ) -> Result<Vec<Comment>, ApiError> {
        let offset = (page - 1) * per_page;
        
        let query = "
            SELECT * FROM comment 
            WHERE parent_id = $parent_id 
            AND is_deleted = false
            ORDER BY created_at ASC
            LIMIT $limit START $offset
        ";

        let replies: Vec<Comment> = self.db.client
            .query(query)
            .bind(("parent_id", Thing::new("comment", parent_id)))
            .bind(("limit", per_page))
            .bind(("offset", offset))
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?
            .take(0)
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        Ok(replies)
    }

    pub async fn get_comment_replies_count(&self, parent_id: &str) -> Result<i64, ApiError> {
        let query = "
            SELECT count() FROM comment 
            WHERE parent_id = $parent_id 
            AND is_deleted = false
            GROUP ALL
        ";

        let result: Vec<serde_json::Value> = self.db.client
            .query(query)
            .bind(("parent_id", Thing::new("comment", parent_id)))
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?
            .take(0)
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        let count = result
            .first()
            .and_then(|v| v.get("count"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        Ok(count)
    }

    pub async fn toggle_comment_like(
        &self,
        comment_id: &str,
        user_id: &str,
    ) -> Result<Comment, ApiError> {
        let mut comment = self.get_comment(comment_id).await?;
        
        if comment.liked_by.contains(&user_id.to_string()) {
            comment.unlike(user_id.to_string());
        } else {
            comment.like(user_id.to_string());
        }

        let updated: Option<Comment> = self.db.client
            .update(("comment", comment_id))
            .content(comment)
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        updated.ok_or_else(|| ApiError::InternalServerError("Failed to update comment".to_string()))
    }

    pub async fn get_comment_thread(&self, comment_id: &str) -> Result<Vec<Comment>, ApiError> {
        let query = "
            SELECT * FROM comment 
            WHERE parent_id = $comment_id 
            AND is_deleted = false
            ORDER BY created_at ASC
        ";

        let thread: Vec<Comment> = self.db.client
            .query(query)
            .bind(("comment_id", Thing::new("comment", comment_id)))
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?
            .take(0)
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        Ok(thread)
    }

    pub async fn search_comments(
        &self,
        document_id: &str,
        query: &str,
        page: i64,
        per_page: i64,
    ) -> Result<Vec<Comment>, ApiError> {
        let offset = (page - 1) * per_page;
        
        let search_query = "
            SELECT * FROM comment 
            WHERE document_id = $document_id 
            AND is_deleted = false
            AND content CONTAINSTEXT $query
            ORDER BY created_at DESC
            LIMIT $limit START $offset
        ";

        let comments: Vec<Comment> = self.db.client
            .query(search_query)
            .bind(("document_id", Thing::new("document", document_id)))
            .bind(("query", query))
            .bind(("limit", per_page))
            .bind(("offset", offset))
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?
            .take(0)
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        Ok(comments)
    }
}
