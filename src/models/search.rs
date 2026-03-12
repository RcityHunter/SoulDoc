use serde::{Deserialize, Serialize};
use surrealdb::types::{Datetime, RecordId as Thing};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchIndex {
    pub id: Option<Thing>,
    pub document_id: Thing,
    pub space_id: Thing,
    pub title: String,
    pub content: String,
    pub excerpt: String,
    pub tags: Vec<String>,
    pub author_id: String,
    pub last_updated: Datetime,
    pub is_public: bool,
}

#[derive(Debug, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    pub space_id: Option<String>,
    pub tags: Option<Vec<String>>,
    pub author_id: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
    pub sort_by: Option<SearchSortBy>,
}

#[derive(Debug, Deserialize)]
pub enum SearchSortBy {
    Relevance,
    CreatedAt,
    UpdatedAt,
    Title,
}

#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub document_id: String,
    pub space_id: String,
    pub title: String,
    pub excerpt: String,
    pub tags: Vec<String>,
    pub author_id: String,
    pub last_updated: Datetime,
    pub score: f64,
    pub highlights: Vec<SearchHighlight>,
}

#[derive(Debug, Serialize)]
pub struct SearchHighlight {
    pub field: String,
    pub text: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    pub total_count: i64,
    pub page: i64,
    pub per_page: i64,
    pub total_pages: i64,
    pub query: String,
    pub took: i64,
}

impl SearchIndex {
    pub fn new(
        document_id: Thing,
        space_id: Thing,
        title: String,
        content: String,
        excerpt: String,
        author_id: String,
    ) -> Self {
        Self {
            id: None,
            document_id,
            space_id,
            title,
            content,
            excerpt,
            tags: Vec::new(),
            author_id,
            last_updated: Datetime::default(),
            is_public: false,
        }
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    pub fn set_public(mut self, is_public: bool) -> Self {
        self.is_public = is_public;
        self
    }

    pub fn update_content(&mut self, title: String, content: String, excerpt: String) {
        self.title = title;
        self.content = content;
        self.excerpt = excerpt;
        self.last_updated = Datetime::default();
    }

    pub fn add_tag(&mut self, tag: String) {
        if !self.tags.contains(&tag) {
            self.tags.push(tag);
        }
    }

    pub fn remove_tag(&mut self, tag: &str) {
        self.tags.retain(|t| t != tag);
    }
}

impl SearchRequest {
    pub fn new(query: String) -> Self {
        Self {
            query,
            space_id: None,
            tags: None,
            author_id: None,
            page: Some(1),
            per_page: Some(20),
            sort_by: Some(SearchSortBy::Relevance),
        }
    }

    pub fn with_space(mut self, space_id: String) -> Self {
        self.space_id = Some(space_id);
        self
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = Some(tags);
        self
    }

    pub fn with_pagination(mut self, page: i64, per_page: i64) -> Self {
        self.page = Some(page);
        self.per_page = Some(per_page);
        self
    }

    pub fn with_sort(mut self, sort_by: SearchSortBy) -> Self {
        self.sort_by = Some(sort_by);
        self
    }
}

impl SearchResponse {
    pub fn new(
        results: Vec<SearchResult>,
        total_count: i64,
        page: i64,
        per_page: i64,
        query: String,
        took: i64,
    ) -> Self {
        let total_pages = (total_count + per_page - 1) / per_page;
        Self {
            results,
            total_count,
            page,
            per_page,
            total_pages,
            query,
            took,
        }
    }
}