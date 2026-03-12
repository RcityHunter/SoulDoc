use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
    response::IntoResponse,
    Extension,
};
use std::sync::Arc;
use serde_json::json;

use crate::{
    services::vector::{
        VectorService, VectorData, VectorSearchRequest, BatchGetRequest, 
        BatchVectorRequest, BatchVectorData
    },
    state::AppState,
    error::{AppError, Result},
};

/// 存储文档向量
pub async fn store_document_vector(
    Path(document_id): Path<String>,
    Extension(state): Extension<Arc<AppState>>,
    Json(vector_data): Json<VectorData>,
) -> Result<impl IntoResponse> {
    let vector_service = VectorService::new(state.db.clone());
    
    let response = vector_service
        .store_vector(&document_id, vector_data)
        .await?;
    
    Ok(Json(response))
}

/// 向量相似度搜索
pub async fn vector_search(
    Extension(state): Extension<Arc<AppState>>,
    Json(request): Json<VectorSearchRequest>,
) -> Result<impl IntoResponse> {
    let vector_service = VectorService::new(state.db.clone());
    
    let response = vector_service
        .search_similar(request)
        .await?;
    
    Ok(Json(response))
}

/// 获取文档向量
pub async fn get_document_vectors(
    Path(document_id): Path<String>,
    Extension(state): Extension<Arc<AppState>>,
) -> Result<impl IntoResponse> {
    let vector_service = VectorService::new(state.db.clone());
    
    let response = vector_service
        .get_document_vectors(&document_id)
        .await?;
    
    Ok(Json(response))
}

/// 删除文档向量
pub async fn delete_document_vector(
    Path((document_id, vector_id)): Path<(String, String)>,
    Extension(state): Extension<Arc<AppState>>,
) -> Result<impl IntoResponse> {
    let vector_service = VectorService::new(state.db.clone());
    
    let success = vector_service
        .delete_vector(&vector_id)
        .await?;
    
    Ok(Json(json!({
        "success": success,
        "deleted_vector_id": vector_id
    })))
}

/// 批量获取文档内容
pub async fn batch_get_documents(
    Extension(state): Extension<Arc<AppState>>,
    Json(request): Json<BatchGetRequest>,
) -> Result<impl IntoResponse> {
    let vector_service = VectorService::new(state.db.clone());
    
    let documents = vector_service
        .batch_get_documents(request.document_ids, request.fields)
        .await?;
    
    Ok(Json(json!({
        "documents": documents
    })))
}

/// 批量更新向量
pub async fn batch_update_vectors(
    Extension(state): Extension<Arc<AppState>>,
    Json(request): Json<BatchVectorRequest>,
) -> Result<impl IntoResponse> {
    let vector_service = VectorService::new(state.db.clone());
    
    let vector_ids = vector_service
        .store_vectors_batch(request.vectors)
        .await?;
    
    Ok(Json(json!({
        "success": true,
        "processed": vector_ids.len(),
        "failed": 0,
        "vector_ids": vector_ids
    })))
}