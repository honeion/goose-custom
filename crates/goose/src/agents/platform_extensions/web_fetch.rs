//! WebFetch Extension - Fetch web content and convert to markdown
//!
//! Provides the `web_fetch` tool for retrieving and processing web content.

use crate::agents::extension::PlatformExtensionContext;
use crate::agents::mcp_client::{Error, McpClientTrait};
use anyhow::Result;
use async_trait::async_trait;
use indoc::indoc;
use reqwest::Client;
use rmcp::model::{
    CallToolResult, Content, Implementation, InitializeResult, JsonObject, ListToolsResult,
    ProtocolVersion, ServerCapabilities, Tool, ToolAnnotations, ToolsCapability,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::warn;

pub static EXTENSION_NAME: &str = "web_fetch";

/// Maximum content size to process (100KB like Claude Code)
const MAX_CONTENT_SIZE: usize = 100 * 1024;

/// Default timeout for HTTP requests
const DEFAULT_TIMEOUT_SECS: u64 = 30;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct WebFetchParams {
    /// The URL to fetch content from
    url: String,
    /// The prompt describing what information to extract from the page
    #[serde(default)]
    prompt: Option<String>,
}

pub struct WebFetchClient {
    info: InitializeResult,
    #[allow(dead_code)]
    context: PlatformExtensionContext,
    http_client: Client,
}

impl WebFetchClient {
    pub fn new(context: PlatformExtensionContext) -> Result<Self> {
        let info = InitializeResult {
            protocol_version: ProtocolVersion::V_2025_03_26,
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability {
                    list_changed: Some(false),
                }),
                tasks: None,
                resources: None,
                extensions: None,
                prompts: None,
                completions: None,
                experimental: None,
                logging: None,
            },
            server_info: Implementation {
                name: EXTENSION_NAME.to_string(),
                description: None,
                title: Some("Web Fetch".to_string()),
                version: "1.0.0".to_string(),
                icons: None,
                website_url: None,
            },
            instructions: Some(
                indoc! {r#"
                    Fetch and process web content from URLs.

                    Use this tool to:
                    - Retrieve content from internal web pages
                    - Extract information from documentation sites
                    - Fetch API documentation or wiki pages

                    The tool converts HTML to Markdown for easier processing.
                    Content is truncated to 100KB if larger.
                "#}
                .to_string(),
            ),
        };

        // Build HTTP client with timeout and system proxy support
        let http_client = Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .user_agent("Goose-Agent/1.0")
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to create HTTP client: {}", e))?;

        Ok(Self {
            info,
            context,
            http_client,
        })
    }

    async fn handle_web_fetch(
        &self,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<Content>, String> {
        let args = arguments.ok_or("Missing arguments")?;

        let url = args
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: url")?;

        let prompt = args
            .get("prompt")
            .and_then(|v| v.as_str())
            .map(String::from);

        // Validate URL
        let parsed_url = url::Url::parse(url)
            .map_err(|e| format!("Invalid URL '{}': {}", url, e))?;

        // Only allow http and https schemes
        if !["http", "https"].contains(&parsed_url.scheme()) {
            return Err(format!("Unsupported URL scheme: {}", parsed_url.scheme()));
        }

        // Fetch content
        let response = self
            .http_client
            .get(url)
            .header("Accept", "text/html,application/xhtml+xml,text/plain,text/markdown")
            .send()
            .await
            .map_err(|e| self.format_fetch_error(url, e))?;

        let status = response.status();
        if !status.is_success() {
            return Err(format!(
                "HTTP request failed: {} {}",
                status.as_u16(),
                status.canonical_reason().unwrap_or("Unknown")
            ));
        }

        // Get content type
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("text/html")
            .to_lowercase();

        // Get response body
        let body = response
            .text()
            .await
            .map_err(|e| format!("Failed to read response body: {}", e))?;

        // Convert to markdown if HTML
        let markdown = if content_type.contains("text/html") || content_type.contains("xhtml") {
            self.html_to_markdown(&body)
        } else {
            // Already plain text or markdown
            body
        };

        // Truncate if too large
        let (content, truncated) = self.truncate_content(&markdown, MAX_CONTENT_SIZE);

        // Build result
        let mut result = String::new();

        if let Some(p) = &prompt {
            result.push_str(&format!("## Query\n{}\n\n", p));
        }

        result.push_str(&format!("## Source\n{}\n\n", url));
        result.push_str("## Content\n");
        result.push_str(&content);

        if truncated {
            result.push_str("\n\n---\n*Content truncated (exceeded 100KB limit)*");
        }

        Ok(vec![Content::text(result)])
    }

    fn html_to_markdown(&self, html: &str) -> String {
        // Use fast_html2md (exported as html2md) for conversion
        html2md::rewrite_html(html, false)
    }

    fn truncate_content(&self, content: &str, max_size: usize) -> (String, bool) {
        if content.len() <= max_size {
            (content.to_string(), false)
        } else {
            // Try to truncate at a reasonable boundary (newline or space)
            let truncated = &content[..max_size];
            let boundary = truncated
                .rfind('\n')
                .or_else(|| truncated.rfind(' '))
                .unwrap_or(max_size);

            (content[..boundary].to_string(), true)
        }
    }

    fn format_fetch_error(&self, url: &str, error: reqwest::Error) -> String {
        if error.is_timeout() {
            format!("Request timed out after {}s: {}", DEFAULT_TIMEOUT_SECS, url)
        } else if error.is_connect() {
            format!(
                "Connection failed: {}. This may be due to network restrictions or the server being unavailable.",
                url
            )
        } else if error.is_redirect() {
            format!("Too many redirects: {}", url)
        } else {
            format!("Failed to fetch {}: {}", url, error)
        }
    }

    fn get_tools() -> Vec<Tool> {
        let schema = schemars::schema_for!(WebFetchParams);
        let schema_value = serde_json::to_value(schema).expect("Failed to serialize schema");

        vec![Tool::new(
            "web_fetch".to_string(),
            indoc! {r#"
                Fetch content from a URL and convert HTML to Markdown.

                Use this tool to:
                - Retrieve documentation or wiki pages
                - Fetch content from internal web applications
                - Extract information from web pages

                Parameters:
                - url: The URL to fetch (required)
                - prompt: Description of what information to extract (optional)

                Returns the page content converted to Markdown format.
                Content is truncated to 100KB if larger.

                Note: Only HTTP/HTTPS URLs are supported.
                Connection errors may occur if the URL is blocked or unreachable.
            "#}
            .to_string(),
            schema_value.as_object().unwrap().clone(),
        )
        .annotate(ToolAnnotations {
            title: Some("Fetch Web Content".to_string()),
            read_only_hint: Some(true),
            destructive_hint: Some(false),
            idempotent_hint: Some(true),
            open_world_hint: Some(true),
        })]
    }
}

#[async_trait]
impl McpClientTrait for WebFetchClient {
    async fn list_tools(
        &self,
        _session_id: &str,
        _next_cursor: Option<String>,
        _cancellation_token: CancellationToken,
    ) -> Result<ListToolsResult, Error> {
        Ok(ListToolsResult {
            tools: Self::get_tools(),
            next_cursor: None,
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        _session_id: &str,
        name: &str,
        arguments: Option<JsonObject>,
        _working_dir: Option<&str>,
        _cancellation_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        let content = match name {
            "web_fetch" => self.handle_web_fetch(arguments).await,
            _ => Err(format!("Unknown tool: {}", name)),
        };

        match content {
            Ok(content) => Ok(CallToolResult::success(content)),
            Err(error) => {
                warn!("WebFetch error: {}", error);
                Ok(CallToolResult::error(vec![Content::text(format!(
                    "Error: {}",
                    error
                ))]))
            }
        }
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        Some(&self.info)
    }

    async fn get_moim(&self, _session_id: &str) -> Option<String> {
        None
    }
}
