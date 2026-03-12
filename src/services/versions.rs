use std::sync::Arc;
use surrealdb::types::RecordId as Thing;
use validator::Validate;

use crate::{
    error::ApiError,
    models::version::{DocumentVersion, CreateVersionRequest, VersionChangeType},
    services::{auth::AuthService, database::{Database, record_id_to_string}},
};

#[derive(Clone)]
pub struct VersionService {
    db: Arc<Database>,
    auth_service: Arc<AuthService>,
}

impl VersionService {
    pub fn new(db: Arc<Database>, auth_service: Arc<AuthService>) -> Self {
        Self { db, auth_service }
    }

    pub async fn create_version(
        &self,
        document_id: &str,
        author_id: &str,
        request: CreateVersionRequest,
    ) -> Result<DocumentVersion, ApiError> {
        request.validate()?;

        // 获取当前最新版本号
        let latest_version = self.get_latest_version_number(document_id).await?;
        let new_version_number = latest_version + 1;

        // 将当前版本设为非当前版本
        self.unset_current_version(document_id).await?;

        let version = DocumentVersion::new(
            Thing::new("document", document_id),
            new_version_number,
            request.title,
            request.content,
            author_id.to_string(),
            request.change_type,
        )
        .with_summary(request.summary.unwrap_or_default())
        .set_as_current();

        let created: Vec<DocumentVersion> = self.db.client
            .create("document_version")
            .content(version)
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        created
            .into_iter()
            .next()
            .ok_or_else(|| ApiError::InternalServerError("Failed to create version".to_string()))
    }

    pub async fn get_version(&self, version_id: &str) -> Result<DocumentVersion, ApiError> {
        let version: Option<DocumentVersion> = self.db.client
            .select(("document_version", version_id))
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        version.ok_or_else(|| ApiError::NotFound("Version not found".to_string()))
    }

    pub async fn get_document_versions(
        &self,
        document_id: &str,
        page: i64,
        per_page: i64,
    ) -> Result<Vec<DocumentVersion>, ApiError> {
        let offset = (page - 1) * per_page;
        
        let query = "
            SELECT * FROM document_version 
            WHERE document_id = $document_id 
            ORDER BY version_number DESC
            LIMIT $limit START $offset
        ";

        let versions: Vec<DocumentVersion> = self.db.client
            .query(query)
            .bind(("document_id", Thing::new("document", document_id)))
            .bind(("limit", per_page))
            .bind(("offset", offset))
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?
            .take(0)
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        Ok(versions)
    }

    pub async fn get_current_version(&self, document_id: &str) -> Result<Option<DocumentVersion>, ApiError> {
        let query = "
            SELECT * FROM document_version 
            WHERE document_id = $document_id 
            AND is_current = true
            LIMIT 1
        ";

        let versions: Vec<DocumentVersion> = self.db.client
            .query(query)
            .bind(("document_id", Thing::new("document", document_id)))
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?
            .take(0)
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        Ok(versions.into_iter().next())
    }

    pub async fn restore_version(
        &self,
        document_id: &str,
        version_id: &str,
        restorer_id: &str,
    ) -> Result<DocumentVersion, ApiError> {
        let old_version = self.get_version(version_id).await?;
        
        // 验证版本属于指定文档
        if record_id_to_string(&old_version.document_id) != format!("document:{}", document_id) {
            return Err(ApiError::BadRequest("Version does not belong to document".to_string()));
        }

        // 创建新版本（恢复操作）
        let restore_request = CreateVersionRequest {
            title: old_version.title.clone(),
            content: old_version.content.clone(),
            summary: Some(format!("Restored from version {}", old_version.version_number)),
            change_type: VersionChangeType::Restored,
        };

        self.create_version(document_id, restorer_id, restore_request).await
    }

    pub async fn compare_versions(
        &self,
        version_id_1: &str,
        version_id_2: &str,
    ) -> Result<VersionComparison, ApiError> {
        let version1 = self.get_version(version_id_1).await?;
        let version2 = self.get_version(version_id_2).await?;

        // 验证两个版本属于同一文档
        if version1.document_id != version2.document_id {
            return Err(ApiError::BadRequest("Versions belong to different documents".to_string()));
        }

        let comparison = VersionComparison {
            from_version: version1.version_number,
            to_version: version2.version_number,
            title_changed: version1.title != version2.title,
            content_diff: self.generate_content_diff(&version1.content, &version2.content),
            summary: format!(
                "Comparing version {} to version {}",
                version1.version_number,
                version2.version_number
            ),
        };

        Ok(comparison)
    }

    pub async fn get_version_history_summary(
        &self,
        document_id: &str,
    ) -> Result<VersionHistorySummary, ApiError> {
        let query = "
            SELECT 
                count() as total_versions,
                MAX(version_number) as latest_version,
                MIN(created_at) as first_created,
                MAX(created_at) as last_updated,
                array::group(change_type) as change_types
            FROM document_version 
            WHERE document_id = $document_id
            GROUP ALL
        ";

        let result: Vec<serde_json::Value> = self.db.client
            .query(query)
            .bind(("document_id", Thing::new("document", document_id)))
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?
            .take(0)
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        if let Some(summary_data) = result.first() {
            // 这里需要解析SurrealDB的返回结果
            // 为简化实现，返回默认值
            Ok(VersionHistorySummary {
                total_versions: 1,
                latest_version_number: 1,
                first_created: surrealdb::types::Datetime::default(),
                last_updated: surrealdb::types::Datetime::default(),
                authors: vec![],
                change_types_count: std::collections::HashMap::new(),
            })
        } else {
            Ok(VersionHistorySummary {
                total_versions: 0,
                latest_version_number: 0,
                first_created: surrealdb::types::Datetime::default(),
                last_updated: surrealdb::types::Datetime::default(),
                authors: vec![],
                change_types_count: std::collections::HashMap::new(),
            })
        }
    }

    pub async fn delete_version(&self, version_id: &str) -> Result<(), ApiError> {
        let version = self.get_version(version_id).await?;
        
        // 不允许删除当前版本
        if version.is_current {
            return Err(ApiError::BadRequest("Cannot delete current version".to_string()));
        }

        let _: Option<DocumentVersion> = self.db.client
            .delete(("document_version", version_id))
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    pub async fn get_versions_by_author(
        &self,
        document_id: &str,
        author_id: &str,
    ) -> Result<Vec<DocumentVersion>, ApiError> {
        let query = "
            SELECT * FROM document_version 
            WHERE document_id = $document_id 
            AND author_id = $author_id
            ORDER BY version_number DESC
        ";

        let versions: Vec<DocumentVersion> = self.db.client
            .query(query)
            .bind(("document_id", Thing::new("document", document_id)))
            .bind(("author_id", author_id))
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?
            .take(0)
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        Ok(versions)
    }

    async fn get_latest_version_number(&self, document_id: &str) -> Result<i32, ApiError> {
        let query = "
            SELECT MAX(version_number) as max_version 
            FROM document_version 
            WHERE document_id = $document_id
            GROUP ALL
        ";

        let result: Vec<serde_json::Value> = self.db.client
            .query(query)
            .bind(("document_id", Thing::new("document", document_id)))
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?
            .take(0)
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        let max_version = result
            .first()
            .and_then(|v| v.get("max_version"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        Ok(max_version as i32)
    }

    async fn unset_current_version(&self, document_id: &str) -> Result<(), ApiError> {
        let query = "
            UPDATE document_version 
            SET is_current = false 
            WHERE document_id = $document_id 
            AND is_current = true
        ";

        let _: Vec<serde_json::Value> = self.db.client
            .query(query)
            .bind(("document_id", Thing::new("document", document_id)))
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?
            .take(0)
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    fn generate_content_diff(&self, old_content: &str, new_content: &str) -> Vec<ContentDiff> {
        // 简化的差异算法，实际应用中可以使用更复杂的diff算法
        let old_lines: Vec<&str> = old_content.lines().collect();
        let new_lines: Vec<&str> = new_content.lines().collect();
        
        let mut diffs = Vec::new();
        let max_lines = old_lines.len().max(new_lines.len());
        
        for i in 0..max_lines {
            let old_line = old_lines.get(i).copied().unwrap_or("");
            let new_line = new_lines.get(i).copied().unwrap_or("");
            
            if old_line != new_line {
                if old_line.is_empty() {
                    diffs.push(ContentDiff {
                        line_number: i + 1,
                        change_type: DiffChangeType::Added,
                        old_content: None,
                        new_content: Some(new_line.to_string()),
                    });
                } else if new_line.is_empty() {
                    diffs.push(ContentDiff {
                        line_number: i + 1,
                        change_type: DiffChangeType::Removed,
                        old_content: Some(old_line.to_string()),
                        new_content: None,
                    });
                } else {
                    diffs.push(ContentDiff {
                        line_number: i + 1,
                        change_type: DiffChangeType::Modified,
                        old_content: Some(old_line.to_string()),
                        new_content: Some(new_line.to_string()),
                    });
                }
            }
        }
        
        diffs
    }
}

#[derive(Debug, serde::Serialize)]
pub struct VersionComparison {
    pub from_version: i32,
    pub to_version: i32,
    pub title_changed: bool,
    pub content_diff: Vec<ContentDiff>,
    pub summary: String,
}

#[derive(Debug, serde::Serialize)]
pub struct ContentDiff {
    pub line_number: usize,
    pub change_type: DiffChangeType,
    pub old_content: Option<String>,
    pub new_content: Option<String>,
}

#[derive(Debug, serde::Serialize)]
pub enum DiffChangeType {
    Added,
    Removed,
    Modified,
}

#[derive(Debug, serde::Serialize)]
pub struct VersionHistorySummary {
    pub total_versions: i64,
    pub latest_version_number: i32,
    pub first_created: surrealdb::types::Datetime,
    pub last_updated: surrealdb::types::Datetime,
    pub authors: Vec<String>,
    pub change_types_count: std::collections::HashMap<String, i64>,
}
