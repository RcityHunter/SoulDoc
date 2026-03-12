use axum::{routing::get, Json, Router};
use std::sync::Arc;

use crate::{services::auth::User, AppState};

pub fn router() -> Router {
    Router::new().route("/me", get(me))
}

async fn me(user: User) -> Json<User> {
    Json(user)
}
