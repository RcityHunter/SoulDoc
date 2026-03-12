use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use surrealdb::types::RecordId as Thing;
use validator::Validate;
use std::collections::HashMap;
use crate::services::database::record_id_to_string;

/// 数据库中的空间发布记录（使用Thing类型的ID）  
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpacePublicationDb {
    pub id: Option<Thing>,
    pub space_id: String,
    pub slug: String,
    pub version: u32,
    pub title: String,
    pub description: Option<String>,
    pub cover_image: Option<String>,
    pub theme: String,
    
    // 发布设置
    pub include_private_docs: bool,
    pub enable_search: bool,
    pub enable_comments: bool,
    pub custom_css: Option<String>,
    pub custom_js: Option<String>,
    
    // SEO 设置
    pub seo_title: Option<String>,
    pub seo_description: Option<String>,
    pub seo_keywords: Vec<String>,
    
    // 状态和时间戳
    pub is_active: bool,
    pub is_deleted: bool,
    pub published_by: String,
    pub published_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
}

/// 空间发布记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpacePublication {
    pub id: Option<String>,
    pub space_id: String,
    pub slug: String,
    pub version: u32,
    pub title: String,
    pub description: Option<String>,
    pub cover_image: Option<String>,
    pub theme: String,
    
    // 发布设置
    pub include_private_docs: bool,
    pub enable_search: bool,
    pub enable_comments: bool,
    pub custom_css: Option<String>,
    pub custom_js: Option<String>,
    
    // SEO 设置
    pub seo_title: Option<String>,
    pub seo_description: Option<String>,
    pub seo_keywords: Vec<String>,
    
    // 状态和时间戳
    pub is_active: bool,
    pub is_deleted: bool,
    pub published_by: String,
    pub published_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
}

/// 发布主题
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PublicationTheme {
    Default,
    Dark,
    Minimal,
}

impl Default for PublicationTheme {
    fn default() -> Self {
        PublicationTheme::Default
    }
}

/// 数据库中的发布文档快照（使用Thing类型的ID）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicationDocumentDb {
    pub id: Option<Thing>,
    pub publication_id: String,
    pub original_doc_id: String,
    
    // 文档内容快照
    pub title: String,
    pub slug: String,
    pub content: String,
    pub excerpt: Option<String>,
    
    // 文档结构
    pub parent_id: Option<String>,
    pub order_index: u32,
    
    // 文档元数据
    pub word_count: u32,
    pub reading_time: u32,
    
    pub created_at: Option<DateTime<Utc>>,
}

/// 发布的文档快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicationDocument {
    pub id: Option<String>,
    pub publication_id: String,
    pub original_doc_id: String,
    
    // 文档内容快照
    pub title: String,
    pub slug: String,
    pub content: String,
    pub excerpt: Option<String>,
    
    // 文档结构
    pub parent_id: Option<String>,
    pub order_index: u32,
    
    // 文档元数据
    pub word_count: u32,
    pub reading_time: u32,
    
    pub created_at: Option<DateTime<Utc>>,
}

/// 数据库中的发布访问统计（使用Thing类型的ID）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicationAnalyticsDb {
    pub id: Option<Thing>,
    pub publication_id: String,
    
    // 访问统计
    pub total_views: u64,
    pub unique_visitors: u64,
    
    // 按时间段统计
    pub views_today: u32,
    pub views_week: u32,
    pub views_month: u32,
    
    // 最热门文档
    pub popular_documents: Vec<PopularDocument>,
    
    pub updated_at: Option<DateTime<Utc>>,
}

/// 发布访问统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicationAnalytics {
    pub id: Option<String>,
    pub publication_id: String,
    
    // 访问统计
    pub total_views: u64,
    pub unique_visitors: u64,
    
    // 按时间段统计
    pub views_today: u32,
    pub views_week: u32,
    pub views_month: u32,
    
    // 最热门文档
    pub popular_documents: Vec<PopularDocument>,
    
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PopularDocument {
    pub doc_id: String,
    pub title: String,
    pub views: u64,
}

/// 自定义域名
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicationDomain {
    pub id: Option<String>,
    pub publication_id: String,
    pub domain: String,
    
    // SSL 证书信息
    pub ssl_status: SslStatus,
    pub ssl_issued_at: Option<DateTime<Utc>>,
    pub ssl_expires_at: Option<DateTime<Utc>>,
    
    // 状态
    pub is_verified: bool,
    pub is_active: bool,
    
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SslStatus {
    Pending,
    Active,
    Failed,
}

/// 数据库中的发布历史记录（使用Thing类型的ID）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicationHistoryDb {
    pub id: Option<Thing>,
    pub publication_id: String,
    pub version: u32,
    
    // 变更信息
    pub change_summary: Option<String>,
    pub changed_documents: Vec<ChangedDocument>,
    
    // 操作信息
    pub published_by: String,
    pub published_at: Option<DateTime<Utc>>,
}

/// 发布历史记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicationHistory {
    pub id: Option<String>,
    pub publication_id: String,
    pub version: u32,
    
    // 变更信息
    pub change_summary: Option<String>,
    pub changed_documents: Vec<ChangedDocument>,
    
    // 操作信息
    pub published_by: String,
    pub published_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangedDocument {
    pub doc_id: String,
    pub title: String,
    pub change_type: ChangeType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeType {
    Added,
    Modified,
    Deleted,
}

/// 创建发布请求
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreatePublicationRequest {
    #[validate(length(min = 1, max = 100))]
    #[validate(regex = "SLUG_REGEX")]
    pub slug: String,
    
    #[validate(length(min = 1, max = 200))]
    pub title: String,
    
    #[validate(length(max = 1000))]
    pub description: Option<String>,
    
    pub cover_image: Option<String>,
    pub theme: Option<String>,
    
    // 发布设置
    pub include_private_docs: Option<bool>,
    pub enable_search: Option<bool>,
    pub enable_comments: Option<bool>,
    pub custom_css: Option<String>,
    pub custom_js: Option<String>,
    
    // SEO 设置
    pub seo_title: Option<String>,
    pub seo_description: Option<String>,
    pub seo_keywords: Option<Vec<String>>,
}

/// 更新发布请求
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdatePublicationRequest {
    #[validate(length(min = 1, max = 200))]
    pub title: Option<String>,
    
    #[validate(length(max = 1000))]
    pub description: Option<String>,
    
    pub cover_image: Option<String>,
    pub theme: Option<String>,
    
    // 发布设置
    pub enable_search: Option<bool>,
    pub enable_comments: Option<bool>,
    pub custom_css: Option<String>,
    pub custom_js: Option<String>,
    
    // SEO 设置
    pub seo_title: Option<String>,
    pub seo_description: Option<String>,
    pub seo_keywords: Option<Vec<String>>,
}

/// 发布响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicationResponse {
    pub id: String,
    pub space_id: String,
    pub slug: String,
    pub version: u32,
    pub title: String,
    pub description: Option<String>,
    pub cover_image: Option<String>,
    pub theme: String,
    
    // URLs
    pub public_url: String,
    pub preview_url: String,
    pub custom_domain: Option<String>,
    
    // 统计信息
    pub document_count: u32,
    pub total_views: u64,
    
    // 状态
    pub is_active: bool,
    pub published_by: String,
    pub published_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 发布文档树节点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicationDocumentNode {
    pub id: String,
    pub title: String,
    pub slug: String,
    pub excerpt: Option<String>,
    pub order_index: u32,
    pub children: Vec<PublicationDocumentNode>,
}

// 正则表达式验证
lazy_static::lazy_static! {
    static ref SLUG_REGEX: regex::Regex = regex::Regex::new(r"^[a-z0-9]+(?:-[a-z0-9]+)*$").unwrap();
}

impl CreatePublicationRequest {
    pub fn validate(&self) -> Result<(), validator::ValidationErrors> {
        Validate::validate(self)
    }
}

impl UpdatePublicationRequest {
    pub fn validate(&self) -> Result<(), validator::ValidationErrors> {
        Validate::validate(self)
    }
}

impl From<SpacePublicationDb> for SpacePublication {
    fn from(db: SpacePublicationDb) -> Self {
        Self {
            id: db.id.map(|thing| record_id_to_string(&thing)),
            space_id: db.space_id,
            slug: db.slug,
            version: db.version,
            title: db.title,
            description: db.description,
            cover_image: db.cover_image,
            theme: db.theme,
            include_private_docs: db.include_private_docs,
            enable_search: db.enable_search,
            enable_comments: db.enable_comments,
            custom_css: db.custom_css,
            custom_js: db.custom_js,
            seo_title: db.seo_title,
            seo_description: db.seo_description,
            seo_keywords: db.seo_keywords,
            is_active: db.is_active,
            is_deleted: db.is_deleted,
            published_by: db.published_by,
            published_at: db.published_at,
            updated_at: db.updated_at,
            deleted_at: db.deleted_at,
        }
    }
}

impl From<PublicationDocumentDb> for PublicationDocument {
    fn from(db: PublicationDocumentDb) -> Self {
        Self {
            id: db.id.map(|thing| record_id_to_string(&thing)),
            publication_id: db.publication_id,
            original_doc_id: db.original_doc_id,
            title: db.title,
            slug: db.slug,
            content: db.content,
            excerpt: db.excerpt,
            parent_id: db.parent_id,
            order_index: db.order_index,
            word_count: db.word_count,
            reading_time: db.reading_time,
            created_at: db.created_at,
        }
    }
}

impl From<PublicationAnalyticsDb> for PublicationAnalytics {
    fn from(db: PublicationAnalyticsDb) -> Self {
        Self {
            id: db.id.map(|thing| record_id_to_string(&thing)),
            publication_id: db.publication_id,
            total_views: db.total_views,
            unique_visitors: db.unique_visitors,
            views_today: db.views_today,
            views_week: db.views_week,
            views_month: db.views_month,
            popular_documents: db.popular_documents,
            updated_at: db.updated_at,
        }
    }
}

impl From<PublicationHistoryDb> for PublicationHistory {
    fn from(db: PublicationHistoryDb) -> Self {
        Self {
            id: db.id.map(|thing| record_id_to_string(&thing)),
            publication_id: db.publication_id,
            version: db.version,
            change_summary: db.change_summary,
            changed_documents: db.changed_documents,
            published_by: db.published_by,
            published_at: db.published_at,
        }
    }
}

impl SpacePublication {
    /// 生成公开访问URL
    pub fn get_public_url(&self, base_url: &str) -> String {
        format!("{}/p/{}", base_url, self.slug)
    }
    
    /// 生成预览URL
    pub fn get_preview_url(&self, base_url: &str) -> String {
        format!("{}/preview/{}", base_url, self.id.as_ref().unwrap_or(&String::new()))
    }
    
    /// 检查是否可以被更新
    pub fn can_update(&self) -> bool {
        self.is_active && !self.is_deleted
    }
}
