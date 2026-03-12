use std::sync::Arc;
use chrono::Utc;
use serde_json::json;
use surrealdb::types::RecordId as Thing;
use tracing::{info, error};

use crate::{
    error::{AppError, Result},
    models::{
        notification::{
            Notification, NotificationDb, CreateNotificationRequest,
            UpdateNotificationRequest, NotificationListQuery, NotificationType,
        },
    },
    services::{database::Database, auth::{AuthService, User}},
    config::Config,
};

pub struct NotificationService {
    db: Arc<Database>,
    auth_service: Arc<AuthService>,
    config: Config,
}

impl NotificationService {
    pub fn new(db: Arc<Database>, auth_service: Arc<AuthService>, config: Config) -> Self {
        Self { db, auth_service, config }
    }

    /// 创建通知
    pub async fn create_notification(&self, request: CreateNotificationRequest) -> Result<Notification> {
        let query = r#"
            CREATE notification SET
                user_id = $user_id,
                type = $type,
                title = $title,
                content = $content,
                data = $data,
                is_read = false,
                created_at = time::now(),
                updated_at = time::now()
        "#;

        let mut result = self.db.client
            .query(query)
            .bind(("user_id", &request.user_id))
            .bind(("type", &request.notification_type))
            .bind(("title", &request.title))
            .bind(("content", &request.content))
            .bind(("data", &request.data))
            .await
            .map_err(|e| {
                error!("Failed to create notification: {}", e);
                AppError::Database(e)
            })?;

        let created: Vec<NotificationDb> = result.take(0)
            .map_err(|e| {
                error!("Failed to retrieve created notification: {}", e);
                AppError::Database(e.into())
            })?;

        let notification = created.into_iter().next()
            .ok_or_else(|| {
                error!("No notification was created for user {}", request.user_id);
                AppError::Internal(anyhow::anyhow!("Failed to create notification"))
            })?;

        info!("Created notification for user {}: {}", request.user_id, request.title);

        Ok(notification.into())
    }

    /// 获取用户通知列表
    pub async fn get_user_notifications(&self, user_id: &str, query_params: NotificationListQuery) -> Result<(Vec<Notification>, u64)> {
        let page = query_params.page.unwrap_or(1).max(1);
        let limit = query_params.limit.unwrap_or(20).min(100);
        let offset = (page - 1) * limit;

        // 构建查询条件
        let mut where_clause = "WHERE user_id = $user_id".to_string();
        if query_params.unread_only.unwrap_or(false) {
            where_clause.push_str(" AND is_read = false");
        }

        // 查询通知
        let query = format!(
            "SELECT * FROM notification {} ORDER BY created_at DESC LIMIT {} START {}",
            where_clause, limit, offset
        );

        let notifications: Vec<NotificationDb> = self.db.client
            .query(&query)
            .bind(("user_id", user_id))
            .await
            .map_err(|e| {
                error!("Failed to query notifications: {}", e);
                AppError::Database(e)
            })?
            .take(0)?;

        // 调试：记录查询到的通知
        for notification in &notifications {
            info!("Retrieved notification - ID: {:?}, data: {:?}", 
                  notification.id, notification.data);
        }

        // 查询总数
        let count_query = format!(
            "SELECT count() as total FROM notification {} GROUP ALL",
            where_clause
        );

        let total_rows: Vec<serde_json::Value> = self.db.client
            .query(&count_query)
            .bind(("user_id", user_id))
            .await
            .map_err(|e| AppError::Database(e))?
            .take(0)?;

        let total = total_rows
            .first()
            .and_then(|v| v.get("total"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        Ok((
            notifications.into_iter().map(Into::into).collect(),
            total,
        ))
    }

    /// 标记通知为已读
    pub async fn mark_as_read(&self, user_id: &str, notification_id: &str) -> Result<Notification> {
        let query = r#"
            UPDATE notification SET
                is_read = true,
                read_at = time::now(),
                updated_at = time::now()
            WHERE id = $id AND user_id = $user_id
        "#;

        let mut result = self.db.client
            .query(query)
            .bind(("id", Thing::new("notification", notification_id)))
            .bind(("user_id", user_id))
            .await
            .map_err(|e| {
                error!("Failed to mark notification as read: {}", e);
                AppError::Database(e)
            })?;

        let updated: Vec<NotificationDb> = result
            .take(0)
            .map_err(|e| {
                error!("Failed to take updated notification: {}", e);
                AppError::Database(e.into())
            })?;

        let notification = updated.into_iter().next()
            .ok_or_else(|| AppError::NotFound("Notification not found".to_string()))?;

        Ok(notification.into())
    }

    /// 标记所有通知为已读
    pub async fn mark_all_as_read(&self, user_id: &str) -> Result<u64> {
        let query = r#"
            UPDATE notification SET
                is_read = true,
                read_at = time::now(),
                updated_at = time::now()
            WHERE user_id = $user_id AND is_read = false
        "#;

        let result: Vec<NotificationDb> = self.db.client
            .query(query)
            .bind(("user_id", user_id))
            .await
            .map_err(|e| AppError::Database(e))?
            .take(0)?;

        Ok(result.len() as u64)
    }

    /// 获取未读通知数量
    pub async fn get_unread_count(&self, user_id: &str) -> Result<u64> {
        let query = "SELECT count() as total FROM notification WHERE user_id = $user_id AND is_read = false GROUP ALL";

        let total_rows: Vec<serde_json::Value> = self.db.client
            .query(query)
            .bind(("user_id", user_id))
            .await
            .map_err(|e| AppError::Database(e))?
            .take(0)?;

        let total = total_rows
            .first()
            .and_then(|v| v.get("total"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        Ok(total)
    }

    /// 删除通知
    pub async fn delete_notification(&self, user_id: &str, notification_id: &str) -> Result<()> {
        let query = "DELETE notification:$id WHERE user_id = $user_id";

        self.db.client
            .query(query)
            .bind(("id", notification_id))
            .bind(("user_id", user_id))
            .await
            .map_err(|e| AppError::Database(e))?;

        Ok(())
    }

    /// 发送空间邀请通知和邮件
    pub async fn send_space_invitation_notification(
        &self,
        to_email: Option<&str>,
        to_user_id: Option<&str>,
        space_name: &str,
        inviter_name: &str,
        invite_token: &str,
        role: &str,
        message: Option<&str>,
        expires_in_days: u64,
    ) -> Result<()> {
        // 如果提供了用户ID，创建站内通知
        if let Some(user_id) = to_user_id {
            let notification_data = json!({
                "space_name": space_name,
                "invite_token": invite_token,
                "role": role,
                "inviter_name": inviter_name,
            });

            self.create_notification(CreateNotificationRequest {
                user_id: user_id.to_string(),
                notification_type: NotificationType::SpaceInvitation,
                title: format!("{} 邀请您加入 {} 空间", inviter_name, space_name),
                content: format!(
                    "{} 邀请您以 {} 的身份加入 {} 空间。{}",
                    inviter_name,
                    role,
                    space_name,
                    message.unwrap_or(""),
                ),
                data: Some(notification_data),
            }).await?;
        }

        // 如果提供了邮箱，发送邮件通知
        if let Some(email) = to_email {
            // 调用 Rainbow-Auth 的邮件服务
            /* let rainbow_auth_url = self.config.auth.rainbow_auth_url
                .as_ref()
                .ok_or_else(|| AppError::Configuration("Rainbow-Auth URL not configured".to_string()))?;

            let url = format!("{}/api/internal/email/notification", rainbow_auth_url);

            let email_data = json!({
                "to": email,
                "notification_type": "space_invitation",
                "data": {
                    "space_name": space_name,
                    "inviter_name": inviter_name,
                    "invite_token": invite_token,
                    "role": role,
                    "message": message,
                    "expires_in_days": expires_in_days,
                }
            });

            let client = reqwest::Client::new();
            let response = client
                .post(&url)
                .header("X-Internal-API-Key", "todo-implement-api-key") // TODO: 实现内部API密钥
                .json(&email_data)
                .send()
                .await
                .map_err(|e| AppError::External(format!("Failed to send email: {}", e)))?;

            if !response.status().is_success() {
                let error_text = response.text().await.unwrap_or_default();
                error!("Failed to send email notification: {}", error_text);
                return Err(AppError::External(format!("Email service error: {}", error_text)));
            } */
        }

        Ok(())
    }
}
