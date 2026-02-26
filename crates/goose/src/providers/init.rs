use std::sync::{Arc, RwLock};

use super::{
    azure::AzureProvider,
    base::{Provider, ProviderMetadata},
    ollama::OllamaProvider,
    provider_registry::ProviderRegistry,
};
use crate::config::ExtensionConfig;
use crate::model::ModelConfig;
use crate::providers::base::ProviderType;
use anyhow::Result;
use tokio::sync::OnceCell;

static REGISTRY: OnceCell<RwLock<ProviderRegistry>> = OnceCell::const_new();

async fn init_registry() -> RwLock<ProviderRegistry> {
    let registry = ProviderRegistry::new().with_providers(|registry| {
        // Azure OpenAI - 주력 Provider
        registry.register::<AzureProvider>(false);

        // Ollama - 로컬 테스트용
        registry.register::<OllamaProvider>(true);

        // 참고: OpenAiCompatibleProvider는 ProviderDef 미구현
        // Azure, Ollama가 내부적으로 사용하는 헬퍼 구조체임
    });

    // declarative providers 비활성화 (외부 서비스)
    // if let Err(e) = load_custom_providers_into_registry(&mut registry) {
    //     tracing::warn!("Failed to load custom providers: {}", e);
    // }

    RwLock::new(registry)
}

async fn get_registry() -> &'static RwLock<ProviderRegistry> {
    REGISTRY.get_or_init(init_registry).await
}

pub async fn providers() -> Vec<(ProviderMetadata, ProviderType)> {
    get_registry()
        .await
        .read()
        .unwrap()
        .all_metadata_with_types()
}

pub async fn refresh_custom_providers() -> Result<()> {
    // declarative providers 비활성화
    tracing::info!("Custom providers disabled in internal build");
    Ok(())
}

async fn get_from_registry(name: &str) -> Result<crate::providers::provider_registry::ProviderEntry> {
    let guard = get_registry().await.read().unwrap();
    guard
        .entries
        .get(name)
        .ok_or_else(|| anyhow::anyhow!("Unknown provider: {}", name))
        .cloned()
}

pub async fn create(
    name: &str,
    model: ModelConfig,
    extensions: Vec<ExtensionConfig>,
) -> Result<Arc<dyn Provider>> {
    let constructor = get_from_registry(name).await?.constructor.clone();
    constructor(model, extensions).await
}

pub async fn create_with_default_model(
    name: impl AsRef<str>,
    extensions: Vec<ExtensionConfig>,
) -> Result<Arc<dyn Provider>> {
    get_from_registry(name.as_ref())
        .await?
        .create_with_default_model(extensions)
        .await
}

pub async fn create_with_named_model(
    provider_name: &str,
    model_name: &str,
    extensions: Vec<ExtensionConfig>,
) -> Result<Arc<dyn Provider>> {
    let config = ModelConfig::new(model_name)?.with_canonical_limits(provider_name);
    create(provider_name, config, extensions).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_azure_provider_registered() {
        let providers_list = providers().await;
        let azure = providers_list.iter().find(|(m, _)| m.name == "azure");
        assert!(azure.is_some(), "Azure provider should be registered");
    }

    #[tokio::test]
    async fn test_ollama_provider_registered() {
        let providers_list = providers().await;
        let ollama = providers_list.iter().find(|(m, _)| m.name == "ollama");
        assert!(ollama.is_some(), "Ollama provider should be registered");
    }
}
