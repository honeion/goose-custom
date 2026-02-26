// Provider 모듈 - Azure OpenAI 전용으로 정리됨
// 원본: https://github.com/block/goose (Apache 2.0)
// 삭제된 Provider는 docs/001-cleanup-plan.md 참조

// === 유지: Azure OpenAI ===
pub mod azure;
pub mod azureauth;

// === 유지: 확장성 (내부 LLM 서버용) ===
pub mod openai_compatible;

// === 유지: 로컬 테스트용 ===
pub mod ollama;

// === 유지: 인프라 ===
pub mod api_client;
pub mod base;
pub mod canonical;
pub mod catalog;
pub mod embedding;
pub mod errors;
pub mod formats;
mod init;
pub mod provider_registry;
pub mod provider_test;
mod retry;
pub mod testprovider;
pub mod toolshim;
pub mod usage_estimator;
pub mod utils;

pub use init::{
    create, create_with_default_model, create_with_named_model, providers, refresh_custom_providers,
};
pub use retry::{retry_operation, RetryConfig};
