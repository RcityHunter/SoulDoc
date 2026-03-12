use serde::{Deserialize, Serialize};
use surrealdb::types::{Datetime, RecordId as Thing};
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentVersion {
    pub id: Option<Thing>,
    pub document_id: Thing,
    pub version_number: i32,
    pub title: String,
    pub content: String,
    pub summary: Option<String>,
    pub author_id: String,
    pub created_at: Datetime,
    pub is_current: bool,
    pub change_type: VersionChangeType,
    pub parent_version_id: Option<Thing>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VersionChangeType {
    Created,
    Updated,
    Restored,
    Merged,
}

#[derive(Debug, Validate, Deserialize)]
pub struct CreateVersionRequest {
    #[validate(length(min = 1, max = 255))]
    pub title: String,
    #[validate(length(min = 1))]
    pub content: String,
    #[validate(length(max = 500))]
    pub summary: Option<String>,
    pub change_type: VersionChangeType,
}

impl DocumentVersion {
    pub fn new(
        document_id: Thing,
        version_number: i32,
        title: String,
        content: String,
        author_id: String,
        change_type: VersionChangeType,
    ) -> Self {
        Self {
            id: None,
            document_id,
            version_number,
            title,
            content,
            summary: None,
            author_id,
            created_at: Datetime::default(),
            is_current: false,
            change_type,
            parent_version_id: None,
        }
    }

    pub fn with_summary(mut self, summary: String) -> Self {
        self.summary = Some(summary);
        self
    }

    pub fn with_parent_version(mut self, parent_version_id: Thing) -> Self {
        self.parent_version_id = Some(parent_version_id);
        self
    }

    pub fn set_as_current(mut self) -> Self {
        self.is_current = true;
        self
    }
}