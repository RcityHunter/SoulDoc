use serde::{Deserialize, Serialize};
use surrealdb::types::{Datetime, RecordId as Thing};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentPermission {
    pub id: Option<Thing>,
    pub resource_type: ResourceType,
    pub resource_id: Thing,
    pub user_id: Option<String>,
    pub role_id: Option<String>,
    pub permissions: Vec<String>,
    pub granted_by: String,
    pub granted_at: Datetime,
    pub expires_at: Option<Datetime>,
    pub is_inherited: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResourceType {
    Space,
    Document,
    Comment,
}

#[derive(Debug, Deserialize)]
pub struct GrantPermissionRequest {
    pub resource_type: ResourceType,
    pub resource_id: String,
    pub user_id: Option<String>,
    pub role_id: Option<String>,
    pub permissions: Vec<String>,
    pub expires_at: Option<Datetime>,
}

#[derive(Debug, Serialize)]
pub struct UserPermissions {
    pub user_id: String,
    pub space_permissions: HashMap<String, Vec<String>>,
    pub document_permissions: HashMap<String, Vec<String>>,
    pub inherited_permissions: Vec<String>,
}

impl DocumentPermission {
    pub fn new(
        resource_type: ResourceType,
        resource_id: Thing,
        permissions: Vec<String>,
        granted_by: String,
    ) -> Self {
        Self {
            id: None,
            resource_type,
            resource_id,
            user_id: None,
            role_id: None,
            permissions,
            granted_by,
            granted_at: Datetime::default(),
            expires_at: None,
            is_inherited: false,
        }
    }

    pub fn for_user(mut self, user_id: String) -> Self {
        self.user_id = Some(user_id);
        self
    }

    pub fn for_role(mut self, role_id: String) -> Self {
        self.role_id = Some(role_id);
        self
    }

    pub fn with_expiry(mut self, expires_at: Datetime) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    pub fn as_inherited(mut self) -> Self {
        self.is_inherited = true;
        self
    }

    pub fn is_expired(&self) -> bool {
        self.expires_at
            .as_ref()
            .map_or(false, |expiry| *expiry < Datetime::default())
    }

    pub fn has_permission(&self, permission: &str) -> bool {
        !self.is_expired() && self.permissions.contains(&permission.to_string())
    }
}

impl UserPermissions {
    pub fn new(user_id: String) -> Self {
        Self {
            user_id,
            space_permissions: HashMap::new(),
            document_permissions: HashMap::new(),
            inherited_permissions: Vec::new(),
        }
    }

    pub fn add_space_permission(&mut self, space_id: String, permissions: Vec<String>) {
        self.space_permissions.insert(space_id, permissions);
    }

    pub fn add_document_permission(&mut self, document_id: String, permissions: Vec<String>) {
        self.document_permissions.insert(document_id, permissions);
    }

    pub fn add_inherited_permissions(&mut self, permissions: Vec<String>) {
        self.inherited_permissions.extend(permissions);
    }

    pub fn has_permission_for_resource(&self, resource_type: ResourceType, resource_id: &str, permission: &str) -> bool {
        match resource_type {
            ResourceType::Space => {
                self.space_permissions
                    .get(resource_id)
                    .map_or(false, |perms| perms.contains(&permission.to_string()))
            }
            ResourceType::Document => {
                self.document_permissions
                    .get(resource_id)
                    .map_or(false, |perms| perms.contains(&permission.to_string()))
            }
            ResourceType::Comment => {
                self.inherited_permissions.contains(&permission.to_string())
                    || self.inherited_permissions.contains(&"docs.comment.manage".to_string())
            }
        }
    }
}