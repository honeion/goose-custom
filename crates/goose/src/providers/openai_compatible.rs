use anyhow::Error;
use async_stream::try_stream;
use futures::TryStreamExt;
use reqwest::{Response, StatusCode};
use serde_json::Value;
use std::time::Instant;
use tokio::pin;
use tokio_stream::StreamExt;
use tokio_util::codec::{FramedRead, LinesCodec};
use tokio_util::io::StreamReader;

use super::api_client::ApiClient;
use super::base::{MessageStream, Provider};
use super::errors::ProviderError;
use super::retry::ProviderRetry;
use super::utils::{ImageFormat, RequestLog};
use crate::audit::{AuditLogger, ApiRequestData, ApiResponseData, TokenUsage};
use crate::conversation::message::Message;
use crate::model::ModelConfig;
use crate::providers::formats::openai::{create_request, response_to_streaming_message};
use rmcp::model::Tool;

pub struct OpenAiCompatibleProvider {
    name: String,
    /// Client targeted at the base URL (e.g. `https://api.x.ai/v1`)
    api_client: ApiClient,
    model: ModelConfig,
    /// Path prefix prepended to `chat/completions` (e.g. `"deployments/{name}/"` for Azure).
    completions_prefix: String,
}

impl OpenAiCompatibleProvider {
    pub fn new(
        name: String,
        api_client: ApiClient,
        model: ModelConfig,
        completions_prefix: String,
    ) -> Self {
        Self {
            name,
            api_client,
            model,
            completions_prefix,
        }
    }

    fn build_request(
        &self,
        model_config: &ModelConfig,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
        for_streaming: bool,
    ) -> Result<Value, ProviderError> {
        create_request(
            model_config,
            system,
            messages,
            tools,
            &ImageFormat::OpenAi,
            for_streaming,
        )
        .map_err(|e| ProviderError::RequestFailed(format!("Failed to create request: {}", e)))
    }
}

#[async_trait::async_trait]
impl Provider for OpenAiCompatibleProvider {
    fn get_name(&self) -> &str {
        &self.name
    }

    fn get_model_config(&self) -> ModelConfig {
        self.model.clone()
    }

    async fn fetch_supported_models(&self) -> Result<Vec<String>, ProviderError> {
        let response = self
            .api_client
            .response_get(None, "models")
            .await
            .map_err(|e| ProviderError::RequestFailed(e.to_string()))?;
        let json = handle_response_openai_compat(response).await?;

        if let Some(err_obj) = json.get("error") {
            let msg = err_obj
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            return Err(ProviderError::Authentication(msg.to_string()));
        }

        let arr = json.get("data").and_then(|v| v.as_array()).ok_or_else(|| {
            ProviderError::RequestFailed("Missing 'data' array in models response".to_string())
        })?;
        let mut models: Vec<String> = arr
            .iter()
            .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(str::to_string))
            .collect();
        models.sort();
        Ok(models)
    }

    async fn stream(
        &self,
        model_config: &ModelConfig,
        session_id: &str,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<MessageStream, ProviderError> {
        let payload = self.build_request(model_config, system, messages, tools, true)?;
        let mut log = RequestLog::start(model_config, &payload)?;

        let request_start = Instant::now();

        // 감사 로그: API 요청
        if let Some(logger) = AuditLogger::global() {
            let message_preview = messages
                .last()
                .map(|m| m.as_concat_text())
                .unwrap_or_default();
            let preview = if message_preview.len() > 200 {
                let mut end = 200;
                while end > 0 && !message_preview.is_char_boundary(end) {
                    end -= 1;
                }
                format!("{}...", &message_preview[..end])
            } else {
                message_preview
            };

            let tool_names: Vec<String> = tools.iter().map(|t| t.name.to_string()).collect();

            logger.log_api_request(session_id, ApiRequestData {
                provider: self.name.clone(),
                model: model_config.model_name.clone(),
                endpoint: Some(format!("{}chat/completions", self.completions_prefix)),
                message_preview: preview,
                tools: tool_names,
                token_estimate: None,
                pii_masked: true, // 이 시점에서는 이미 마스킹됨
                masked_tokens: vec![],
            });
        }

        let completions_path = format!("{}chat/completions", self.completions_prefix);
        let response = self
            .with_retry(|| async {
                let resp = self
                    .api_client
                    .response_post(Some(session_id), &completions_path, &payload)
                    .await?;
                handle_status_openai_compat(resp).await
            })
            .await
            .inspect_err(|e| {
                let _ = log.error(e);
            })?;

        let latency_ms = request_start.elapsed().as_millis() as u64;
        stream_openai_compat(response, log, session_id.to_string(), latency_ms)
    }
}

fn check_context_length_exceeded(text: &str) -> bool {
    let check_phrases = [
        "too long",
        "context length",
        "context_length_exceeded",
        "reduce the length",
        "token count",
        "exceeds",
        "exceed context limit",
        "input length",
        "max_tokens",
        "decrease input length",
        "context limit",
        "maximum prompt length",
    ];
    let text_lower = text.to_lowercase();
    check_phrases
        .iter()
        .any(|phrase| text_lower.contains(phrase))
}

pub fn map_http_error_to_provider_error(
    status: StatusCode,
    payload: Option<Value>,
) -> ProviderError {
    let extract_message = || -> String {
        payload
            .as_ref()
            .and_then(|p| {
                p.get("error")
                    .and_then(|e| e.get("message"))
                    .or_else(|| p.get("message"))
                    .and_then(|m| m.as_str())
                    .map(String::from)
            })
            .unwrap_or_else(|| payload.as_ref().map(|p| p.to_string()).unwrap_or_default())
    };

    let error = match status {
        StatusCode::OK => unreachable!("Should not call this function with OK status"),
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => ProviderError::Authentication(format!(
            "Authentication failed. Status: {}. Response: {}",
            status,
            extract_message()
        )),
        StatusCode::NOT_FOUND => {
            ProviderError::RequestFailed(format!("Resource not found (404): {}", extract_message()))
        }
        StatusCode::PAYMENT_REQUIRED => ProviderError::CreditsExhausted {
            details: extract_message(),
            top_up_url: None,
        },
        StatusCode::PAYLOAD_TOO_LARGE => ProviderError::ContextLengthExceeded(extract_message()),
        StatusCode::BAD_REQUEST => {
            let payload_str = extract_message();
            if check_context_length_exceeded(&payload_str) {
                ProviderError::ContextLengthExceeded(payload_str)
            } else {
                ProviderError::RequestFailed(format!("Bad request (400): {}", payload_str))
            }
        }
        StatusCode::TOO_MANY_REQUESTS => ProviderError::RateLimitExceeded {
            details: extract_message(),
            retry_delay: None,
        },
        _ if status.is_server_error() => {
            ProviderError::ServerError(format!("Server error ({}): {}", status, extract_message()))
        }
        _ => ProviderError::RequestFailed(format!(
            "Request failed with status {}: {}",
            status,
            extract_message()
        )),
    };

    if !status.is_success() {
        tracing::warn!(
            "Provider request failed with status: {}. Payload: {:?}. Returning error: {:?}",
            status,
            payload,
            error
        );
    }

    error
}

pub async fn handle_status_openai_compat(response: Response) -> Result<Response, ProviderError> {
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let payload = serde_json::from_str::<Value>(&body).ok();
        return Err(map_http_error_to_provider_error(status, payload));
    }
    Ok(response)
}

pub async fn handle_response_openai_compat(response: Response) -> Result<Value, ProviderError> {
    let response = handle_status_openai_compat(response).await?;

    response.json::<Value>().await.map_err(|e| {
        ProviderError::RequestFailed(format!("Response body is not valid JSON: {}", e))
    })
}

pub fn stream_openai_compat(
    response: Response,
    mut log: RequestLog,
    session_id: String,
    latency_ms: u64,
) -> Result<MessageStream, ProviderError> {
    let stream = response.bytes_stream().map_err(std::io::Error::other);

    Ok(Box::pin(try_stream! {
        let stream_reader = StreamReader::new(stream);
        let framed = FramedRead::new(stream_reader, LinesCodec::new())
            .map_err(Error::from);

        let message_stream = response_to_streaming_message(framed);
        pin!(message_stream);

        let mut total_input_tokens = 0u64;
        let mut total_output_tokens = 0u64;

        while let Some(message) = message_stream.next().await {
            let (message, usage) = message.map_err(|e|
                ProviderError::RequestFailed(format!("Stream decode error: {}", e))
            )?;

            // 토큰 사용량 누적
            if let Some(ref u) = usage {
                total_input_tokens = u.usage.input_tokens.unwrap_or(0) as u64;
                total_output_tokens = u.usage.output_tokens.unwrap_or(0) as u64;
            }

            log.write(&message, usage.as_ref().map(|f| f.usage).as_ref())?;
            yield (message, usage);
        }

        // 감사 로그: API 응답
        if let Some(logger) = AuditLogger::global() {
            logger.log_api_response(&session_id, ApiResponseData {
                usage: TokenUsage {
                    input: total_input_tokens,
                    output: total_output_tokens,
                },
                latency_ms,
                tool_calls: vec![], // 도구 호출은 별도 이벤트로 기록
                response_preview: None,
                error: None,
            });
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use test_case::test_case;

    #[test_case(
        StatusCode::PAYMENT_REQUIRED,
        Some(json!({"error": {"message": "Insufficient credits to complete this request"}})),
        "CreditsExhausted"
        ; "402 with payload"
    )]
    #[test_case(
        StatusCode::PAYMENT_REQUIRED,
        None,
        "CreditsExhausted"
        ; "402 without payload"
    )]
    #[test_case(
        StatusCode::TOO_MANY_REQUESTS,
        Some(json!({"error": {"message": "Rate limit exceeded"}})),
        "RateLimitExceeded"
        ; "429 rate limit"
    )]
    #[test_case(
        StatusCode::UNAUTHORIZED,
        None,
        "Authentication"
        ; "401 unauthorized"
    )]
    #[test_case(
        StatusCode::BAD_REQUEST,
        Some(json!({"error": {"message": "This request exceeds the maximum context length"}})),
        "ContextLengthExceeded"
        ; "400 context length"
    )]
    #[test_case(
        StatusCode::INTERNAL_SERVER_ERROR,
        None,
        "ServerError"
        ; "500 server error"
    )]
    fn http_status_maps_to_expected_error(
        status: StatusCode,
        payload: Option<Value>,
        expected_variant: &str,
    ) {
        let err = map_http_error_to_provider_error(status, payload);
        let actual = err.telemetry_type();
        let expected_telemetry = match expected_variant {
            "CreditsExhausted" => "credits_exhausted",
            "RateLimitExceeded" => "rate_limit",
            "Authentication" => "auth",
            "ContextLengthExceeded" => "context_length",
            "ServerError" => "server",
            other => panic!("Unknown variant: {other}"),
        };
        assert_eq!(
            actual, expected_telemetry,
            "Expected {expected_variant}, got error: {err:?}"
        );
    }
}
