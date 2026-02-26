// Setup routes - 내부망 버전 (외부 OAuth 서비스 제거)
// OpenRouter, Tetrate 인증 제거됨
// 수동 설정만 지원

use crate::routes::errors::ErrorResponse;
use crate::state::AppState;
use axum::{routing::get, Json, Router};
use serde::Serialize;
use std::sync::Arc;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
pub struct SetupResponse {
    pub success: bool,
    pub message: String,
}

pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/status", get(setup_status))
        .with_state(state)
}

#[utoipa::path(
    get,
    path = "/status",
    responses(
        (status = 200, body=SetupResponse)
    ),
)]
async fn setup_status() -> Result<Json<SetupResponse>, ErrorResponse> {
    // 내부망 버전: 외부 OAuth 서비스 비활성화
    Ok(Json(SetupResponse {
        success: true,
        message: "Internal build: Use manual configuration (goose configure)".to_string(),
    }))
}
