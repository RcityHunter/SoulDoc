use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::RecordId as Thing;
use crate::services::database::record_id_to_string;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationType {
    SpaceInvitation,
    DocumentShared,
    CommentMention,
    DocumentUpdate,
    System,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NotificationDb {
    pub id: Option<Thing>,
    pub user_id: String,
    #[serde(rename = "type")]
    pub notification_type: NotificationType,
    pub title: String,
    pub content: String,
    pub data: Option<serde_json::Value>,
    pub invite_token: Option<String>,
    pub space_name: Option<String>,
    pub role: Option<String>,
    pub inviter_name: Option<String>,
    pub is_read: bool,
    pub read_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub id: Option<String>,
    pub user_id: String,
    #[serde(rename = "type")]
    pub notification_type: NotificationType,
    pub title: String,
    pub content: String,
    pub data: Option<serde_json::Value>,
    pub invite_token: Option<String>,
    pub space_name: Option<String>,
    pub role: Option<String>,
    pub inviter_name: Option<String>,
    pub is_read: bool,
    pub read_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateNotificationRequest {
    pub user_id: String,
    #[serde(rename = "type")]
    pub notification_type: NotificationType,
    pub title: String,
    pub content: String,
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateNotificationRequest {
    pub is_read: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NotificationListQuery {
    pub page: Option<u32>,
    pub limit: Option<u32>,
    pub unread_only: Option<bool>,
}

impl From<NotificationDb> for Notification {
    fn from(db: NotificationDb) -> Self {
        Self {
            id: db.id.map(|thing| record_id_to_string(&thing)),
            user_id: db.user_id,
            notification_type: db.notification_type,
            title: db.title,
            content: db.content,
            data: db.data,
            invite_token: db.invite_token,
            space_name: db.space_name,
            role: db.role,
            inviter_name: db.inviter_name,
            is_read: db.is_read,
            read_at: db.read_at,
            created_at: db.created_at,
            updated_at: db.updated_at,
        }
    }
}
