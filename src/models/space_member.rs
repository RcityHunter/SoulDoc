use crate::services::database::{record_id_key, record_id_to_string};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::RecordId as Thing;
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FlexibleRecordId {
    RecordId(Thing),
    String(String),
}

impl FlexibleRecordId {
    pub fn into_string(self) -> String {
        match self {
            FlexibleRecordId::RecordId(thing) => record_id_to_string(&thing),
            FlexibleRecordId::String(value) => value,
        }
    }

    pub fn into_key_string(self) -> String {
        match self {
            FlexibleRecordId::RecordId(thing) => record_id_key(&thing),
            FlexibleRecordId::String(value) => value
                .strip_prefix("space:")
                .or_else(|| value.strip_prefix("space_invitation:"))
                .unwrap_or(&value)
                .to_string(),
        }
    }
}

// 用于从数据库读取的内部结构
#[derive(Debug, Clone, Deserialize)]
pub struct SpaceMemberDb {
    pub id: Option<Thing>,
    pub space_id: Thing,
    pub user_id: String,
    pub role: MemberRole,
    pub permissions: Vec<String>,
    pub invited_by: String,
    pub invited_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
    pub status: MemberStatus,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceMember {
    pub id: Option<String>,
    pub space_id: String,
    pub user_id: String,
    pub role: MemberRole,
    pub permissions: Vec<String>,
    pub invited_by: String,
    pub invited_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
    pub status: MemberStatus,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MemberRole {
    Owner,
    Admin,
    Editor,
    Viewer,
    Member,
}

impl ToString for MemberRole {
    fn to_string(&self) -> String {
        match self {
            MemberRole::Owner => "所有者".to_string(),
            MemberRole::Admin => "管理员".to_string(),
            MemberRole::Editor => "编辑者".to_string(),
            MemberRole::Viewer => "查看者".to_string(),
            MemberRole::Member => "成员".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MemberStatus {
    Pending,
    Accepted,
    Rejected,
    Removed,
}

impl Default for MemberRole {
    fn default() -> Self {
        MemberRole::Member
    }
}

impl Default for MemberStatus {
    fn default() -> Self {
        MemberStatus::Pending
    }
}

impl MemberRole {
    pub fn default_permissions(&self) -> Vec<String> {
        match self {
            MemberRole::Owner => vec![
                "docs.read".to_string(),
                "docs.write".to_string(),
                "docs.delete".to_string(),
                "docs.admin".to_string(),
                "space.admin".to_string(),
                "space.delete".to_string(),
                "members.invite".to_string(),
                "members.remove".to_string(),
                "members.manage".to_string(),
            ],
            MemberRole::Admin => vec![
                "docs.read".to_string(),
                "docs.write".to_string(),
                "docs.delete".to_string(),
                "docs.admin".to_string(),
                "members.invite".to_string(),
                "members.manage".to_string(),
            ],
            MemberRole::Editor => vec!["docs.read".to_string(), "docs.write".to_string()],
            MemberRole::Viewer => vec!["docs.read".to_string()],
            MemberRole::Member => vec!["docs.read".to_string(), "docs.write".to_string()],
        }
    }

    pub fn can_perform(&self, permission: &str) -> bool {
        self.default_permissions().contains(&permission.to_string())
    }
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct InviteMemberRequest {
    #[validate(email(message = "Invalid email format"))]
    pub email: Option<String>,

    pub user_id: Option<String>, // 直接通过用户ID邀请

    pub role: MemberRole,

    pub message: Option<String>,

    #[validate(range(
        min = 1,
        max = 365,
        message = "Expiration days must be between 1 and 365"
    ))]
    pub expires_in_days: Option<u32>, // 邀请过期天数，默认7天
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct UpdateMemberRequest {
    pub role: Option<MemberRole>,
    pub permissions: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SpaceMemberResponse {
    pub id: String,
    pub space_id: String,
    pub user_id: String,
    pub role: MemberRole,
    pub permissions: Vec<String>,
    pub status: MemberStatus,
    pub invited_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceInvitationDb {
    pub id: Option<FlexibleRecordId>,
    pub space_id: FlexibleRecordId,
    pub email: Option<String>,
    pub user_id: Option<String>,
    pub invite_token: String,
    pub role: MemberRole,
    pub permissions: Vec<String>,
    pub invited_by: String,
    pub message: Option<String>,
    pub max_uses: u32,
    pub used_count: u32,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceInvitation {
    pub id: Option<String>,
    pub space_id: String,
    pub email: Option<String>,
    pub user_id: Option<String>,
    pub invite_token: String,
    pub role: MemberRole,
    pub permissions: Vec<String>,
    pub invited_by: String,
    pub message: Option<String>,
    pub max_uses: u32,
    pub used_count: u32,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AcceptInvitationRequest {
    pub invite_token: String,
}

impl From<SpaceMemberDb> for SpaceMember {
    fn from(db: SpaceMemberDb) -> Self {
        Self {
            id: db.id.map(|thing| record_id_to_string(&thing)),
            space_id: record_id_key(&db.space_id),
            user_id: db.user_id,
            role: db.role,
            permissions: db.permissions,
            invited_by: db.invited_by,
            invited_at: db.invited_at,
            accepted_at: db.accepted_at,
            status: db.status,
            expires_at: db.expires_at,
            created_at: db.created_at,
            updated_at: db.updated_at,
        }
    }
}

impl From<SpaceMember> for SpaceMemberResponse {
    fn from(member: SpaceMember) -> Self {
        Self {
            id: member.id.unwrap_or_default(),
            space_id: member.space_id,
            user_id: member.user_id,
            role: member.role,
            permissions: member.permissions,
            status: member.status,
            invited_at: member.invited_at,
            accepted_at: member.accepted_at,
            created_at: member.created_at,
            updated_at: member.updated_at,
        }
    }
}

impl From<SpaceInvitationDb> for SpaceInvitation {
    fn from(db: SpaceInvitationDb) -> Self {
        Self {
            id: db.id.map(FlexibleRecordId::into_string),
            space_id: db.space_id.into_key_string(),
            email: db.email,
            user_id: db.user_id,
            invite_token: db.invite_token,
            role: db.role,
            permissions: db.permissions,
            invited_by: db.invited_by,
            message: db.message,
            max_uses: db.max_uses,
            used_count: db.used_count,
            expires_at: db.expires_at,
            created_at: db.created_at,
            updated_at: db.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FlexibleRecordId;

    #[test]
    fn flexible_record_id_accepts_string_record_ids() {
        let id: FlexibleRecordId =
            serde_json::from_str(r#""space_invitation:sa5phk80ljowxr2b14ao""#).unwrap();

        assert_eq!(id.into_string(), "space_invitation:sa5phk80ljowxr2b14ao");
    }

    #[test]
    fn flexible_record_id_extracts_space_key_from_string_record_ids() {
        let id: FlexibleRecordId = serde_json::from_str(r#""space:rc3z5qyduqu824o32szx""#).unwrap();

        assert_eq!(id.into_key_string(), "rc3z5qyduqu824o32szx");
    }
}
