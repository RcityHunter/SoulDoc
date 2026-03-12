use crate::{
    services::database::Database,
    error::{ApiError, Result},
    models::{
        publication::*,
        document::{Document, DocumentTreeNode},
    },
};
use surrealdb::types::RecordId as Thing;
use chrono::Utc;
use std::sync::Arc;
use tracing::{info, warn, error};

pub struct PublicationService {
    db: Arc<Database>,
}

impl PublicationService {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// 创建新的发布
    pub async fn create_publication(
        &self,
        space_id: &str,
        publisher_id: &str,
        request: CreatePublicationRequest,
    ) -> Result<PublicationResponse> {
        // 验证请求
        request.validate()
            .map_err(|e| ApiError::Validation(e.to_string()))?;

        // 检查slug是否已被使用
        if self.slug_exists(&request.slug).await? {
            return Err(ApiError::Conflict(format!("Slug '{}' already exists", request.slug)));
        }

        // 获取最新版本号
        let latest_version = self.get_latest_version(space_id).await?;
        let new_version = (latest_version + 1) as u32;

        // 创建发布记录
        let publication = SpacePublication {
            id: None,
            space_id: space_id.to_string(),
            slug: request.slug,
            version: new_version,
            title: request.title,
            description: request.description,
            cover_image: request.cover_image,
            theme: request.theme.unwrap_or_else(|| "default".to_string()),
            include_private_docs: request.include_private_docs.unwrap_or(false),
            enable_search: request.enable_search.unwrap_or(true),
            enable_comments: request.enable_comments.unwrap_or(false),
            custom_css: request.custom_css,
            custom_js: request.custom_js,
            seo_title: request.seo_title,
            seo_description: request.seo_description,
            seo_keywords: request.seo_keywords.unwrap_or_default(),
            is_active: true,
            is_deleted: false,
            published_by: publisher_id.to_string(),
            published_at: None,  // 让数据库使用默认值
            updated_at: None,    // 让数据库使用默认值
            deleted_at: None,
        };

        // 保存到数据库
        let created: Vec<SpacePublication> = self.db.client
            .create("space_publication")
            .content(publication)
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        let created_publication = created.into_iter()
            .next()
            .ok_or_else(|| ApiError::InternalServerError("Failed to create publication".to_string()))?;

        let publication_id = created_publication.id.as_ref()
            .ok_or_else(|| ApiError::InternalServerError("Publication ID is missing".to_string()))?;

        // 创建文档快照
        let document_count = self.create_document_snapshots(
            publication_id,
            space_id,
            created_publication.include_private_docs,
        ).await?;

        // 创建发布历史记录
        self.create_publication_history(
            publication_id,
            new_version as i32,
            publisher_id,
            "Initial publication",
        ).await?;

        // 初始化访问统计
        self.init_analytics(publication_id).await?;

        info!("Created publication {} for space {} with {} documents", 
            created_publication.slug, space_id, document_count);

        // 构建响应
        Ok(self.build_publication_response(created_publication, document_count, 0).await?)
    }

    /// 更新现有发布
    pub async fn update_publication(
        &self,
        publication_id: &str,
        updater_id: &str,
        request: UpdatePublicationRequest,
    ) -> Result<PublicationResponse> {
        // 验证请求
        request.validate()
            .map_err(|e| ApiError::Validation(e.to_string()))?;

        // 获取现有发布
        let mut publication = self.get_publication_by_id(publication_id).await?;

        if !publication.can_update() {
            return Err(ApiError::BadRequest("Publication cannot be updated".to_string()));
        }

        // 更新字段
        if let Some(title) = request.title {
            publication.title = title;
        }
        if let Some(description) = request.description {
            publication.description = Some(description);
        }
        if request.cover_image.is_some() {
            publication.cover_image = request.cover_image;
        }
        if let Some(theme) = request.theme {
            publication.theme = theme;
        }
        if let Some(enable_search) = request.enable_search {
            publication.enable_search = enable_search;
        }
        if let Some(enable_comments) = request.enable_comments {
            publication.enable_comments = enable_comments;
        }
        if request.custom_css.is_some() {
            publication.custom_css = request.custom_css;
        }
        if request.custom_js.is_some() {
            publication.custom_js = request.custom_js;
        }
        if request.seo_title.is_some() {
            publication.seo_title = request.seo_title;
        }
        if request.seo_description.is_some() {
            publication.seo_description = request.seo_description;
        }
        if let Some(keywords) = request.seo_keywords {
            publication.seo_keywords = keywords;
        }

        // 更新数据库
        let query = "UPDATE $id SET 
            title = $title,
            description = $description,
            cover_image = $cover_image,
            theme = $theme,
            enable_search = $enable_search,
            enable_comments = $enable_comments,
            custom_css = $custom_css,
            custom_js = $custom_js,
            seo_title = $seo_title,
            seo_description = $seo_description,
            seo_keywords = $seo_keywords,
            updated_at = time::now()";

        self.db.client
            .query(query)
            .bind(("id", self.get_publication_thing(publication_id)))
            .bind(("title", &publication.title))
            .bind(("description", &publication.description))
            .bind(("cover_image", &publication.cover_image))
            .bind(("theme", &publication.theme))
            .bind(("enable_search", publication.enable_search))
            .bind(("enable_comments", publication.enable_comments))
            .bind(("custom_css", &publication.custom_css))
            .bind(("custom_js", &publication.custom_js))
            .bind(("seo_title", &publication.seo_title))
            .bind(("seo_description", &publication.seo_description))
            .bind(("seo_keywords", &publication.seo_keywords))
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        // 获取文档数量和访问统计
        let document_count = self.get_document_count(publication_id).await?;
        let analytics = self.get_analytics(publication_id).await?;

        Ok(self.build_publication_response(publication, document_count, analytics.total_views).await?)
    }

    /// 重新发布（更新文档快照）
    pub async fn republish(
        &self,
        publication_id: &str,
        publisher_id: &str,
        change_summary: Option<String>,
    ) -> Result<PublicationResponse> {
        // 获取现有发布
        let mut publication = self.get_publication_by_id(publication_id).await?;

        if !publication.can_update() {
            return Err(ApiError::BadRequest("Publication cannot be republished".to_string()));
        }

        // 增加版本号
        publication.version += 1;

        // 更新版本号
        let query = "UPDATE $id SET version = $version, updated_at = time::now()";
        self.db.client
            .query(query)
            .bind(("id", self.get_publication_thing(publication_id)))
            .bind(("version", publication.version))
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        // 删除旧的文档快照
        self.delete_document_snapshots(publication_id).await?;

        // 创建新的文档快照
        let document_count = self.create_document_snapshots(
            publication_id,
            &publication.space_id,
            publication.include_private_docs,
        ).await?;

        // 创建发布历史记录
        self.create_publication_history(
            publication_id,
            publication.version as i32,
            publisher_id,
            &change_summary.unwrap_or_else(|| "Content update".to_string()),
        ).await?;

        info!("Republished {} (v{}) with {} documents", 
            publication.slug, publication.version, document_count);

        // 获取访问统计
        let analytics = self.get_analytics(publication_id).await?;

        Ok(self.build_publication_response(publication, document_count, analytics.total_views).await?)
    }

    /// 取消发布
    pub async fn unpublish(&self, publication_id: &str) -> Result<()> {
        let query = "UPDATE $id SET is_active = false, updated_at = time::now()";
        
        self.db.client
            .query(query)
            .bind(("id", self.get_publication_thing(publication_id)))
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        info!("Unpublished publication {}", publication_id);
        Ok(())
    }

    /// 删除发布
    pub async fn delete_publication(&self, publication_id: &str) -> Result<()> {
        info!("Deleting publication: {}", publication_id);
        
        let query = "UPDATE $id SET is_deleted = true, deleted_at = time::now()";
        
        self.db.client
            .query(query)
            .bind(("id", self.get_publication_thing(publication_id)))
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        info!("Deleted publication {}", publication_id);
        Ok(())
    }

    /// 获取空间的所有发布
    pub async fn list_publications(
        &self,
        space_id: &str,
        include_inactive: bool,
    ) -> Result<Vec<PublicationResponse>> {
        let query = if include_inactive {
            "SELECT * FROM space_publication 
            WHERE space_id = $space_id AND is_deleted = false
            ORDER BY version DESC"
        } else {
            "SELECT * FROM space_publication 
            WHERE space_id = $space_id AND is_deleted = false AND is_active = true
            ORDER BY version DESC"
        };

        let mut result = self.db.client
            .query(query)
            .bind(("space_id", space_id))
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        let publications_db: Vec<SpacePublicationDb> = result
            .take(0)
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        let mut responses = Vec::new();
        for pub_item_db in publications_db {
            let pub_item: SpacePublication = pub_item_db.into();
            if let Some(pub_id) = &pub_item.id {
                let document_count = self.get_document_count(pub_id).await?;
                let analytics = self.get_analytics(pub_id).await?;
                responses.push(self.build_publication_response(pub_item, document_count, analytics.total_views).await?);
            }
        }

        Ok(responses)
    }

    /// 获取发布的文档树
    pub async fn get_publication_tree(
        &self,
        publication_id: &str,
    ) -> Result<Vec<PublicationDocumentNode>> {
        info!("Getting publication tree for publication_id: {}", publication_id);
        
        let query = "SELECT * FROM publication_document 
            WHERE publication_id = $publication_id 
            ORDER BY order_index ASC";

        let formatted_id = self.format_publication_id(publication_id);
        info!("Using formatted_id for query: {}", formatted_id);

        let mut result = self.db.client
            .query(query)
            .bind(("publication_id", formatted_id))
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        let documents_db: Vec<PublicationDocumentDb> = result
            .take(0)
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;
            
        let documents: Vec<PublicationDocument> = documents_db.into_iter().map(|db| db.into()).collect();
        info!("Found {} documents in publication_document table", documents.len());
        
        // 打印每个文档的详细信息
        for doc in &documents {
            info!("Published doc: title={}, id={:?}, parent_id={:?}, original_doc_id={}", 
                doc.title, doc.id, doc.parent_id, doc.original_doc_id);
        }

        // 构建树结构
        self.build_document_tree(documents)
    }

    /// 获取发布的单个文档
    pub async fn get_publication_document(
        &self,
        publication_id: &str,
        doc_slug: &str,
    ) -> Result<PublicationDocument> {
        let query = "SELECT * FROM publication_document 
            WHERE publication_id = $publication_id AND slug = $slug";

        let mut result = self.db.client
            .query(query)
            .bind(("publication_id", self.format_publication_id(publication_id)))
            .bind(("slug", doc_slug))
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        let documents_db: Vec<PublicationDocumentDb> = result
            .take(0)
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        documents_db.into_iter()
            .map(|db| db.into())
            .next()
            .ok_or_else(|| ApiError::NotFound("Document not found".to_string()))
    }

    // ===== 私有辅助方法 =====

    /// 格式化 publication_id 为完整的 Thing 格式
    fn format_publication_id(&self, publication_id: &str) -> String {
        if publication_id.starts_with("space_publication:") {
            publication_id.to_string()
        } else {
            format!("space_publication:{}", publication_id)
        }
    }

    /// 获取 Thing 类型的 publication_id
    fn get_publication_thing(&self, publication_id: &str) -> Thing {
        let clean_id = publication_id.strip_prefix("space_publication:").unwrap_or(publication_id);
        Thing::new("space_publication", clean_id)
    }

    /// 检查slug是否已存在
    async fn slug_exists(&self, slug: &str) -> Result<bool> {
        let query = "SELECT count() as total FROM space_publication 
            WHERE slug = $slug AND is_active = true AND is_deleted = false
            GROUP ALL";

        let mut result = self.db.client
            .query(query)
            .bind(("slug", slug))
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        let records: Vec<serde_json::Value> = result
            .take(0)
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        let count = records
            .first()
            .and_then(|v| v.get("total"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        Ok(count > 0)
    }

    /// 获取空间的最新版本号
    async fn get_latest_version(&self, space_id: &str) -> Result<i32> {
        let query = "SELECT version FROM space_publication 
            WHERE space_id = $space_id 
            ORDER BY version DESC LIMIT 1";

        let mut result = self.db.client
            .query(query)
            .bind(("space_id", space_id))
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        let versions: Vec<serde_json::Value> = result
            .take(0)
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        Ok(versions
            .first()
            .and_then(|v| v.get("version"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as i32)
    }

    /// 创建文档快照
    async fn create_document_snapshots(
        &self,
        publication_id: &str,
        space_id: &str,
        include_private: bool,
    ) -> Result<u32> {
        info!("Creating document snapshots for space_id: {}, include_private: {}", space_id, include_private);
        
        // 处理 space_id 格式：去掉 "space:" 前缀
        let clean_space_id = space_id.strip_prefix("space:").unwrap_or(space_id);
        info!("Using clean_space_id for document query: {}", clean_space_id);
        
        // 首先尝试查询所有文档来调试
        let debug_query = "SELECT id, space_id FROM document LIMIT 5";
        let debug_result: Vec<serde_json::Value> = self.db.client
            .query(debug_query)
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?
            .take(0)
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;
        
        info!("Sample documents from database: {:?}", debug_result);
        
        // 获取要发布的文档
        // 注意：space_id 在数据库中是 Thing 类型，需要使用 Thing 进行查询
        let query = if include_private {
            "SELECT * FROM document 
            WHERE space_id = $space_id AND is_deleted = false 
            ORDER BY order_index ASC, created_at ASC"
        } else {
            "SELECT * FROM document 
            WHERE space_id = $space_id AND is_deleted = false AND is_public = true 
            ORDER BY order_index ASC, created_at ASC"
        };
        
        info!("Document query: {}", query);
        info!("Query binding - space_id: space:{}", clean_space_id);

        let mut result = self.db.client
            .query(query)
            .bind(("space_id", Thing::new("space", clean_space_id)))
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        let documents_db: Vec<crate::models::document::DocumentDb> = result
            .take(0)
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;
        
        let documents: Vec<Document> = documents_db.into_iter().map(|db| db.into()).collect();

        let document_count = documents.len() as u32;
        info!("Found {} documents to publish", document_count);
        
        // 打印所有文档的详细信息
        for doc in &documents {
            info!("Document: title={}, id={:?}, parent_id={:?}, is_public={}", 
                doc.title, doc.id, doc.parent_id, doc.is_public);
        }

        // 创建快照
        for doc in documents {
            if let Some(doc_id) = &doc.id {
                info!("Creating snapshot for document: {} ({})", doc.title, doc_id);
                // 创建业务模型用于插入（按照Space服务的模式）
                let snapshot = PublicationDocument {
                    id: None,
                    publication_id: publication_id.to_string(),
                    original_doc_id: doc_id.clone(),
                    title: doc.title.clone(),
                    slug: doc.slug.clone(),
                    content: doc.content.clone(),
                    excerpt: doc.excerpt.clone(),
                    parent_id: doc.parent_id.clone(),  // 保持原始的parent_id格式
                    order_index: doc.order_index as u32,
                    word_count: doc.word_count,
                    reading_time: doc.reading_time,
                    created_at: None,  // 让数据库使用默认值
                };

                let _: Vec<PublicationDocument> = self.db.client
                    .create("publication_document")
                    .content(snapshot)
                    .await
                    .map_err(|e| ApiError::DatabaseError(e.to_string()))?;
            }
        }

        Ok(document_count)
    }

    /// 删除文档快照
    async fn delete_document_snapshots(&self, publication_id: &str) -> Result<()> {
        let query = "DELETE publication_document WHERE publication_id = $publication_id";
        
        self.db.client
            .query(query)
            .bind(("publication_id", self.format_publication_id(publication_id)))
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    /// 创建发布历史记录
    async fn create_publication_history(
        &self,
        publication_id: &str,
        version: i32,
        publisher_id: &str,
        change_summary: &str,
    ) -> Result<()> {
        let history = PublicationHistory {
            id: None,
            publication_id: publication_id.to_string(),
            version: version as u32,
            change_summary: Some(change_summary.to_string()),
            changed_documents: vec![], // TODO: 实现文档变更检测
            published_by: publisher_id.to_string(),
            published_at: None,  // 让数据库使用默认值
        };

        let _: Vec<PublicationHistory> = self.db.client
            .create("publication_history")
            .content(history)
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    /// 初始化访问统计
    async fn init_analytics(&self, publication_id: &str) -> Result<()> {
        let analytics = PublicationAnalytics {
            id: None,
            publication_id: publication_id.to_string(),
            total_views: 0,
            unique_visitors: 0,
            views_today: 0,
            views_week: 0,
            views_month: 0,
            popular_documents: vec![],
            updated_at: None,  // 让数据库使用默认值
        };

        let _: Vec<PublicationAnalytics> = self.db.client
            .create("publication_analytics")
            .content(analytics)
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    /// 获取发布
    pub async fn get_publication_by_id(&self, publication_id: &str) -> Result<SpacePublication> {
        info!("Getting publication by id: {}", publication_id);
        
        // 处理 ID 格式：去掉可能的表前缀
        let clean_id = publication_id.strip_prefix("space_publication:").unwrap_or(publication_id);
        info!("Using clean_id: {}", clean_id);
        
        let publications_db: Option<SpacePublicationDb> = self.db.client
            .query("SELECT * FROM $id WHERE is_deleted = false")
            .bind(("id", Thing::new("space_publication", clean_id)))
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?
            .take(0)
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        publications_db
            .map(|db| db.into())
            .ok_or_else(|| ApiError::NotFound("Publication not found".to_string()))
    }

    /// 通过slug获取发布
    pub async fn get_publication_by_slug(&self, slug: &str) -> Result<SpacePublication> {
        let query = "SELECT * FROM space_publication 
            WHERE slug = $slug AND is_active = true AND is_deleted = false";

        let mut result = self.db.client
            .query(query)
            .bind(("slug", slug))
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        let publications_db: Vec<SpacePublicationDb> = result
            .take(0)
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        publications_db.into_iter()
            .map(|db| db.into())
            .next()
            .ok_or_else(|| ApiError::NotFound("Publication not found".to_string()))
    }

    /// 获取文档数量
    async fn get_document_count(&self, publication_id: &str) -> Result<u32> {
        let query = "SELECT count() as total FROM publication_document 
            WHERE publication_id = $publication_id GROUP ALL";

        let mut result = self.db.client
            .query(query)
            .bind(("publication_id", publication_id))
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        let records: Vec<serde_json::Value> = result
            .take(0)
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        let count = records
            .first()
            .and_then(|v| v.get("total"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        Ok(count)
    }

    /// 获取访问统计
    async fn get_analytics(&self, publication_id: &str) -> Result<PublicationAnalytics> {
        let query = "SELECT * FROM publication_analytics WHERE publication_id = $publication_id";

        let mut result = self.db.client
            .query(query)
            .bind(("publication_id", publication_id))
            .await
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        let analytics_db: Vec<PublicationAnalyticsDb> = result
            .take(0)
            .map_err(|e| ApiError::DatabaseError(e.to_string()))?;

        analytics_db.into_iter()
            .map(|db| db.into())
            .next()
            .ok_or_else(|| ApiError::NotFound("Analytics not found".to_string()))
    }

    /// 构建发布响应
    async fn build_publication_response(
        &self,
        publication: SpacePublication,
        document_count: u32,
        total_views: u64,
    ) -> Result<PublicationResponse> {
        // 使用前端URL来生成预览和公开访问链接
        let frontend_url = std::env::var("FRONTEND_URL").unwrap_or_else(|_| "http://129.226.169.63:4173".to_string());
        
        // 先调用方法获取URL
        let public_url = publication.get_public_url(&frontend_url);
        let preview_url = publication.get_preview_url(&frontend_url);

        Ok(PublicationResponse {
            id: publication.id.clone().unwrap_or_default(),
            space_id: publication.space_id,
            slug: publication.slug.clone(),
            version: publication.version,
            title: publication.title,
            description: publication.description,
            cover_image: publication.cover_image,
            theme: publication.theme,
            public_url,
            preview_url,
            custom_domain: None, // TODO: 实现自定义域名
            document_count,
            total_views,
            is_active: publication.is_active,
            published_by: publication.published_by,
            published_at: publication.published_at.unwrap_or_else(Utc::now),
            updated_at: publication.updated_at.unwrap_or_else(Utc::now),
        })
    }

    /// 构建文档树
    fn build_document_tree(&self, documents: Vec<PublicationDocument>) -> Result<Vec<PublicationDocumentNode>> {
        info!("Building document tree for {} documents", documents.len());
        
        let mut doc_map = std::collections::HashMap::new();
        let mut children_map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
        let mut root_docs = Vec::new();
        
        // 创建原始文档ID到发布文档ID的映射
        let mut original_to_published: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for doc in &documents {
            if let Some(pub_id) = &doc.id {
                original_to_published.insert(doc.original_doc_id.clone(), pub_id.clone());
            }
        }

        // 第一次遍历：创建所有节点并识别父子关系
        for doc in documents {
            if let Some(doc_id) = &doc.id {
                info!("Processing document: {} (id: {}, parent: {:?}, original: {})", 
                    doc.title, doc_id, doc.parent_id, doc.original_doc_id);
                    
                let node = PublicationDocumentNode {
                    id: doc_id.clone(),
                    title: doc.title.clone(),
                    slug: doc.slug.clone(),
                    excerpt: doc.excerpt.clone(),
                    order_index: doc.order_index as u32,
                    children: Vec::new(),
                };
                
                doc_map.insert(doc_id.clone(), node);
                
                if let Some(parent_id) = &doc.parent_id {
                    // 将原始文档的parent_id转换为发布文档的parent_id
                    if let Some(published_parent_id) = original_to_published.get(parent_id) {
                        info!("Mapping parent {} to {}", parent_id, published_parent_id);
                        children_map.entry(published_parent_id.clone())
                            .or_insert_with(Vec::new)
                            .push(doc_id.clone());
                    } else {
                        info!("Warning: parent {} not found in mapping, treating as root", parent_id);
                        root_docs.push(doc_id.clone());
                    }
                } else {
                    root_docs.push(doc_id.clone());
                }
            }
        }

        // 第二次遍历：构建树结构
        fn build_tree_recursive(
            doc_id: &str,
            doc_map: &mut std::collections::HashMap<String, PublicationDocumentNode>,
            children_map: &std::collections::HashMap<String, Vec<String>>,
        ) -> Option<PublicationDocumentNode> {
            if let Some(mut node) = doc_map.remove(doc_id) {
                // 递归构建子节点
                if let Some(child_ids) = children_map.get(doc_id) {
                    for child_id in child_ids {
                        if let Some(child_node) = build_tree_recursive(child_id, doc_map, children_map) {
                            node.children.push(child_node);
                        }
                    }
                    // 按order_index排序子节点
                    node.children.sort_by_key(|child| child.order_index);
                }
                Some(node)
            } else {
                None
            }
        }

        // 构建最终的树结构
        let mut result = Vec::new();
        for root_id in &root_docs {
            if let Some(root_node) = build_tree_recursive(root_id, &mut doc_map, &children_map) {
                result.push(root_node);
            }
        }
        
        // 按order_index排序根节点
        result.sort_by_key(|node| node.order_index);

        Ok(result)
    }

    /// 记录文档访问
    pub async fn track_document_view(
        &self,
        publication_id: &str,
        document_id: &str,
    ) -> Result<()> {
        // TODO: 实现访问统计
        Ok(())
    }
}