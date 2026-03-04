//! Browser Extension - Chrome/Edge automation via CDP
//!
//! Tools:
//! - browser_launch: Launch browser (headless/headed)
//! - browser_navigate: Navigate to URL
//! - browser_click: Click element
//! - browser_input: Input text to element
//! - browser_screenshot: Take screenshot
//! - browser_read_page: Read page content (HTML)
//! - browser_find: Find elements by query
//! - browser_close: Close browser

use chromiumoxide::{Browser, BrowserConfig, Page};
use futures::StreamExt;
use once_cell::sync::Lazy;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Content, ErrorCode, ErrorData, Implementation, ServerCapabilities, ServerInfo},
    schemars::JsonSchema,
    tool, tool_handler, tool_router, ServerHandler,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

// Global browser state - persists across tool calls
static BROWSER_STATE: Lazy<Arc<Mutex<Option<Browser>>>> = Lazy::new(|| Arc::new(Mutex::new(None)));
static PAGE_STATE: Lazy<Arc<Mutex<Option<Page>>>> = Lazy::new(|| Arc::new(Mutex::new(None)));

// =============================================================================
// Tool Parameters
// =============================================================================

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct BrowserLaunchParams {
    /// Run in headless mode (no visible window). Default: false (visible)
    #[serde(default)]
    pub headless: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct BrowserNavigateParams {
    /// URL to navigate to
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct BrowserClickParams {
    /// CSS selector for the element to click
    pub selector: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct BrowserInputParams {
    /// CSS selector for the input element
    pub selector: String,
    /// Text value to input
    pub value: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct BrowserScreenshotParams {
    /// Filename to save screenshot (default: screenshot.png)
    #[serde(default)]
    pub filename: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct BrowserReadPageParams {
    /// Filter: "all" or "interactive" (default: all)
    #[serde(default)]
    pub filter: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct BrowserFindParams {
    /// CSS selector to find elements
    pub selector: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct BrowserCloseParams {}

// =============================================================================
// Browser Extension
// =============================================================================

/// Browser Extension for web automation
#[derive(Clone)]
pub struct BrowserExtension {
    tool_router: ToolRouter<Self>,
}

impl BrowserExtension {
    /// Find Chrome/Edge executable on the system
    fn find_browser_executable() -> Result<PathBuf, ErrorData> {
        #[cfg(target_os = "windows")]
        let candidates = [
            // Chrome first (better chromiumoxide compatibility)
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
            r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        ];

        #[cfg(target_os = "macos")]
        let candidates = [
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        ];

        #[cfg(target_os = "linux")]
        let candidates = [
            "/usr/bin/google-chrome",
            "/usr/bin/google-chrome-stable",
            "/usr/bin/chromium",
            "/usr/bin/chromium-browser",
            "/usr/bin/microsoft-edge",
        ];

        for path in candidates {
            if Path::new(path).exists() {
                tracing::info!("Found browser at: {}", path);
                return Ok(PathBuf::from(path));
            }
        }

        Err(ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            "Chrome/Edge not found. Please install Chrome or Edge browser.".to_string(),
            None,
        ))
    }
}

// =============================================================================
// ServerHandler Implementation
// =============================================================================

#[tool_handler(router = self.tool_router)]
impl ServerHandler for BrowserExtension {
    fn get_info(&self) -> ServerInfo {
        let instructions = r#"Browser automation extension using Chrome DevTools Protocol (CDP).

Available tools:
- browser_launch: Launch Chrome/Edge browser. Set headless=false (default) to watch automation in real-time.
- browser_navigate: Navigate to a URL.
- browser_click: Click element by CSS selector.
- browser_input: Input text into element by CSS selector.
- browser_screenshot: Take screenshot of current page.
- browser_read_page: Read page HTML content.
- browser_find: Find elements by CSS selector.
- browser_close: Close browser and cleanup.

Workflow: browser_launch → browser_navigate → (interact) → browser_close
"#;

        ServerInfo {
            server_info: Implementation {
                name: "browser".to_string(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
                title: None,
                description: None,
                icons: None,
                website_url: None,
            },
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            instructions: Some(instructions.to_string()),
            ..Default::default()
        }
    }
}

// =============================================================================
// Tool Implementations
// =============================================================================

#[tool_router(router = tool_router)]
impl BrowserExtension {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        name = "browser_launch",
        description = "Launch Chrome/Edge browser. Set headless=false (default) to see the browser window and watch automation in real-time. Set headless=true for background execution."
    )]
    pub async fn browser_launch(
        &self,
        params: Parameters<BrowserLaunchParams>,
    ) -> Result<CallToolResult, ErrorData> {
        // Clean up any existing browser state first
        {
            let mut page_guard = PAGE_STATE.lock().await;
            *page_guard = None;
        }
        {
            let mut browser_guard = BROWSER_STATE.lock().await;
            if browser_guard.is_some() {
                tracing::info!("Closing existing browser before launching new one");
                *browser_guard = None;
            }
        }

        let params = params.0;
        let browser_path = Self::find_browser_executable()?;
        let headless = params.headless.unwrap_or(false);

        // Create a unique user data directory to avoid conflicts with existing browser instances
        // Use both process ID and timestamp for uniqueness
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let user_data_dir = std::env::temp_dir().join(format!("goose_browser_{}_{}", std::process::id(), timestamp));

        tracing::info!("Launching browser: {:?}", browser_path);
        tracing::info!("User data dir: {:?}", user_data_dir);

        let mut builder = BrowserConfig::builder()
            .chrome_executable(browser_path)
            .user_data_dir(user_data_dir)
            .arg("--disable-gpu")
            .arg("--no-sandbox")
            .arg("--disable-dev-shm-usage")
            .arg("--remote-debugging-port=0")  // Let browser pick available port
            .arg("--window-size=1920,1080")    // Full HD window
            .arg("--force-device-scale-factor=1")   // 100% zoom
            .arg("--start-maximized")               // Start maximized
            .viewport(chromiumoxide::handler::viewport::Viewport {
                width: 1920,
                height: 1080,
                device_scale_factor: Some(1.0),
                emulating_mobile: false,
                is_landscape: true,
                has_touch: false,
            });

        if !headless {
            builder = builder.with_head();
        }

        let config = builder.build().map_err(|e| {
            ErrorData::new(ErrorCode::INTERNAL_ERROR, format!("Failed to build browser config: {}", e), None)
        })?;

        let (browser, mut handler) = Browser::launch(config).await.map_err(|e| {
            ErrorData::new(ErrorCode::INTERNAL_ERROR, format!("Failed to launch browser: {}", e), None)
        })?;

        tracing::info!("Browser launched successfully, starting handler...");

        // Spawn handler for CDP events - must keep running
        tokio::spawn(async move {
            tracing::info!("CDP handler started");
            loop {
                match handler.next().await {
                    Some(Ok(_)) => {
                        // Event handled successfully
                    }
                    Some(Err(e)) => {
                        tracing::warn!("Browser handler error: {:?}", e);
                    }
                    None => {
                        tracing::info!("Browser handler stream ended");
                        break;
                    }
                }
            }
            tracing::info!("CDP handler exited");
        });

        // Store browser in global state (don't get page here - let navigate handle it)
        *BROWSER_STATE.lock().await = Some(browser);
        tracing::info!("Browser stored in global state");

        let mode = if headless { "headless" } else { "headed (visible)" };
        Ok(CallToolResult::success(vec![Content::text(format!("Browser launched in {} mode.", mode))]))
    }

    #[tool(
        name = "browser_navigate",
        description = "Navigate to a URL. Browser must be launched first with browser_launch."
    )]
    pub async fn browser_navigate(
        &self,
        params: Parameters<BrowserNavigateParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;

        // Check if we already have a page - reuse it instead of creating new tab
        {
            let mut page_guard = PAGE_STATE.lock().await;
            if let Some(existing_page) = page_guard.as_ref() {
                // Navigate existing page to new URL
                existing_page.goto(&params.url).await.map_err(|e| {
                    ErrorData::new(ErrorCode::INTERNAL_ERROR, format!("Failed to navigate: {}", e), None)
                })?;

                // Wait for page to load
                existing_page.wait_for_navigation().await.ok();

                let title = existing_page.get_title().await.unwrap_or_default().unwrap_or_default();
                return Ok(CallToolResult::success(vec![Content::text(format!("Navigated to: {}\nTitle: {}", params.url, title))]));
            }
        }

        // No existing page in state - check if browser has existing tabs first
        let browser_guard = BROWSER_STATE.lock().await;
        let browser = browser_guard.as_ref().ok_or_else(|| {
            ErrorData::new(ErrorCode::INTERNAL_ERROR, "Browser not launched. Call browser_launch first.".to_string(), None)
        })?;

        // Try to get existing tab first (the one chromiumoxide creates by default)
        let page = if let Ok(pages) = browser.pages().await {
            if let Some(existing) = pages.into_iter().next() {
                tracing::info!("Reusing existing browser tab");
                // Navigate existing tab to URL
                existing.goto(&params.url).await.map_err(|e| {
                    ErrorData::new(ErrorCode::INTERNAL_ERROR, format!("Failed to navigate: {}", e), None)
                })?;
                existing
            } else {
                // No existing tabs - create new one
                tracing::info!("No existing tabs, creating new page");
                browser.new_page(&params.url).await.map_err(|e| {
                    ErrorData::new(ErrorCode::INTERNAL_ERROR, format!("Failed to navigate: {}", e), None)
                })?
            }
        } else {
            // Failed to get pages - create new one
            tracing::info!("Could not get pages, creating new page");
            browser.new_page(&params.url).await.map_err(|e| {
                ErrorData::new(ErrorCode::INTERNAL_ERROR, format!("Failed to navigate: {}", e), None)
            })?
        };

        // Wait for page to load
        page.wait_for_navigation().await.ok();

        let title = page.get_title().await.unwrap_or_default().unwrap_or_default();

        drop(browser_guard);
        *PAGE_STATE.lock().await = Some(page);

        Ok(CallToolResult::success(vec![Content::text(format!("Navigated to: {}\nTitle: {}", params.url, title))]))
    }

    #[tool(
        name = "browser_click",
        description = "Click an element by CSS selector. Example selectors: '#submit-btn', '.login-button', 'button[type=submit]', 'a.nav-link'"
    )]
    pub async fn browser_click(
        &self,
        params: Parameters<BrowserClickParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;
        let page_guard = PAGE_STATE.lock().await;
        let page = page_guard.as_ref().ok_or_else(|| {
            ErrorData::new(ErrorCode::INTERNAL_ERROR, "No page open. Call browser_navigate first.".to_string(), None)
        })?;

        page.find_element(&params.selector).await.map_err(|e| {
            ErrorData::new(ErrorCode::INTERNAL_ERROR, format!("Element not found '{}': {}", params.selector, e), None)
        })?.click().await.map_err(|e| {
            ErrorData::new(ErrorCode::INTERNAL_ERROR, format!("Failed to click: {}", e), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(format!("Clicked element: {}", params.selector))]))
    }

    #[tool(
        name = "browser_input",
        description = "Input text into an element by CSS selector. Example: selector='input#username', value='myuser'"
    )]
    pub async fn browser_input(
        &self,
        params: Parameters<BrowserInputParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;
        let page_guard = PAGE_STATE.lock().await;
        let page = page_guard.as_ref().ok_or_else(|| {
            ErrorData::new(ErrorCode::INTERNAL_ERROR, "No page open. Call browser_navigate first.".to_string(), None)
        })?;

        let element = page.find_element(&params.selector).await.map_err(|e| {
            ErrorData::new(ErrorCode::INTERNAL_ERROR, format!("Element not found '{}': {}", params.selector, e), None)
        })?;

        // Clear existing text and type new value
        element.click().await.ok();
        element.type_str(&params.value).await.map_err(|e| {
            ErrorData::new(ErrorCode::INTERNAL_ERROR, format!("Failed to input text: {}", e), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(format!("Input '{}' to element: {}", params.value, params.selector))]))
    }

    #[tool(
        name = "browser_screenshot",
        description = "Take a screenshot of the current page. Default filename: screenshot.png"
    )]
    pub async fn browser_screenshot(
        &self,
        params: Parameters<BrowserScreenshotParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;
        let page_guard = PAGE_STATE.lock().await;
        let page = page_guard.as_ref().ok_or_else(|| {
            ErrorData::new(ErrorCode::INTERNAL_ERROR, "No page open. Call browser_navigate first.".to_string(), None)
        })?;

        let bytes = page.screenshot(chromiumoxide::page::ScreenshotParams::default()).await.map_err(|e| {
            ErrorData::new(ErrorCode::INTERNAL_ERROR, format!("Failed to take screenshot: {}", e), None)
        })?;

        let filename = params.filename.unwrap_or_else(|| "screenshot.png".to_string());
        std::fs::write(&filename, &bytes).map_err(|e| {
            ErrorData::new(ErrorCode::INTERNAL_ERROR, format!("Failed to save screenshot: {}", e), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(format!("Screenshot saved: {}", filename))]))
    }

    #[tool(
        name = "browser_read_page",
        description = "Read the current page HTML content. Use browser_find to locate specific elements by CSS selector."
    )]
    pub async fn browser_read_page(
        &self,
        params: Parameters<BrowserReadPageParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let _params = params.0;
        let page_guard = PAGE_STATE.lock().await;
        let page = page_guard.as_ref().ok_or_else(|| {
            ErrorData::new(ErrorCode::INTERNAL_ERROR, "No page open. Call browser_navigate first.".to_string(), None)
        })?;

        // Get page HTML content
        let content = page.content().await.map_err(|e| {
            ErrorData::new(ErrorCode::INTERNAL_ERROR, format!("Failed to read page: {}", e), None)
        })?;

        // Truncate if too long
        let max_len = 50000;
        let truncated = if content.len() > max_len {
            format!("{}...\n\n[Truncated, {} chars total]", &content[..max_len], content.len())
        } else {
            content
        };

        Ok(CallToolResult::success(vec![Content::text(truncated)]))
    }

    #[tool(
        name = "browser_find",
        description = "Find elements by CSS selector. Returns count of matching elements. Use to verify elements exist before clicking/inputting."
    )]
    pub async fn browser_find(
        &self,
        params: Parameters<BrowserFindParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let params = params.0;
        let page_guard = PAGE_STATE.lock().await;
        let page = page_guard.as_ref().ok_or_else(|| {
            ErrorData::new(ErrorCode::INTERNAL_ERROR, "No page open. Call browser_navigate first.".to_string(), None)
        })?;

        let elements = page.find_elements(&params.selector).await.map_err(|e| {
            ErrorData::new(ErrorCode::INTERNAL_ERROR, format!("Failed to find elements: {}", e), None)
        })?;

        let count = elements.len();
        Ok(CallToolResult::success(vec![Content::text(format!("Found {} elements matching '{}'", count, params.selector))]))
    }

    #[tool(
        name = "browser_close",
        description = "Close the browser and clean up resources. Always call this when done with browser automation."
    )]
    pub async fn browser_close(
        &self,
        _params: Parameters<BrowserCloseParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut page_guard = PAGE_STATE.lock().await;
        *page_guard = None;
        drop(page_guard);

        let mut browser_guard = BROWSER_STATE.lock().await;
        if let Some(browser) = browser_guard.take() {
            // Browser will be dropped and closed
            drop(browser);
        }

        Ok(CallToolResult::success(vec![Content::text("Browser closed".to_string())]))
    }
}

impl Default for BrowserExtension {
    fn default() -> Self {
        Self::new()
    }
}
