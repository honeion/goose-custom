use crate::agents::extension::PlatformExtensionContext;
use crate::agents::mcp_client::{Error, McpClientTrait};
use crate::session::extension_data;
use crate::session::extension_data::{ExtensionState, Task, TaskState, TaskStatus};
use anyhow::Result;
use async_trait::async_trait;
use indoc::indoc;
use rmcp::model::{
    CallToolResult, Content, Implementation, InitializeResult, JsonObject, ListToolsResult,
    ProtocolVersion, ServerCapabilities, Tool, ToolAnnotations, ToolsCapability,
};
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

pub static EXTENSION_NAME: &str = "todo";

// Legacy todo_write params (kept for backward compatibility)
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct TodoWriteParams {
    content: String,
}

// Task management params
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct TaskCreateParams {
    /// Brief title for the task (imperative form, e.g., "Fix login bug")
    subject: String,
    /// Detailed description of what needs to be done
    description: String,
    /// Present continuous form shown in spinner when in_progress (e.g., "Fixing login bug")
    #[serde(default)]
    active_form: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct TaskUpdateParams {
    /// The ID of the task to update
    task_id: String,
    /// New status for the task
    #[serde(default)]
    status: Option<String>,
    /// New subject for the task
    #[serde(default)]
    subject: Option<String>,
    /// New description for the task
    #[serde(default)]
    description: Option<String>,
    /// Present continuous form for spinner
    #[serde(default)]
    active_form: Option<String>,
    /// Task IDs that this task blocks
    #[serde(default)]
    add_blocks: Option<Vec<String>>,
    /// Task IDs that block this task
    #[serde(default)]
    add_blocked_by: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct TaskGetParams {
    /// The ID of the task to retrieve
    task_id: String,
}

pub struct TodoClient {
    info: InitializeResult,
    context: PlatformExtensionContext,
}

impl TodoClient {
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
                title: Some("Task Manager".to_string()),
                version: "2.0.0".to_string(),
                icons: None,
                website_url: None,
            },
            instructions: Some(
                indoc! {r#"
                Use structured task management for complex multi-step work.

                When to use tasks:
                - Complex tasks requiring 3+ steps
                - Multi-file changes
                - Tasks with dependencies

                Workflow:
                1. task_create: Create tasks for each step
                2. task_update: Set status to in_progress when starting
                3. task_update: Set status to completed when done
                4. task_list: Check remaining tasks

                Task statuses: pending → in_progress → completed
            "#}
                .to_string(),
            ),
        };

        Ok(Self { info, context })
    }

    // Helper to get or create TaskState
    async fn get_task_state(&self, session_id: &str) -> Result<(TaskState, extension_data::ExtensionData), String> {
        let manager = &self.context.session_manager;
        let session = manager
            .get_session(session_id, false)
            .await
            .map_err(|_| "Failed to read session")?;

        let task_state = TaskState::from_extension_data(&session.extension_data)
            .unwrap_or_else(TaskState::new);

        Ok((task_state, session.extension_data))
    }

    // Helper to save TaskState
    async fn save_task_state(
        &self,
        session_id: &str,
        task_state: &TaskState,
        mut extension_data: extension_data::ExtensionData,
    ) -> Result<(), String> {
        task_state
            .to_extension_data(&mut extension_data)
            .map_err(|_| "Failed to serialize task state")?;

        self.context
            .session_manager
            .update(session_id)
            .extension_data(extension_data)
            .apply()
            .await
            .map_err(|_| "Failed to save task state")?;

        Ok(())
    }

    async fn handle_task_create(
        &self,
        session_id: &str,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<Content>, String> {
        let args = arguments.ok_or("Missing arguments")?;

        let subject = args
            .get("subject")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: subject")?
            .to_string();

        let description = args
            .get("description")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: description")?
            .to_string();

        let active_form = args
            .get("active_form")
            .and_then(|v| v.as_str())
            .map(String::from);

        let (mut task_state, ext_data) = self.get_task_state(session_id).await?;
        let task = task_state.create_task(subject, description, active_form);
        let task_id = task.id.clone();
        let task_subject = task.subject.clone();

        self.save_task_state(session_id, &task_state, ext_data).await?;

        Ok(vec![Content::text(format!(
            "Created task #{}: {}",
            task_id, task_subject
        ))])
    }

    async fn handle_task_update(
        &self,
        session_id: &str,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<Content>, String> {
        let args = arguments.ok_or("Missing arguments")?;

        let task_id = args
            .get("task_id")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: task_id")?;

        let (mut task_state, ext_data) = self.get_task_state(session_id).await?;

        let task = task_state
            .get_task_mut(task_id)
            .ok_or_else(|| format!("Task #{} not found", task_id))?;

        // Update status
        if let Some(status_str) = args.get("status").and_then(|v| v.as_str()) {
            task.status = match status_str {
                "pending" => TaskStatus::Pending,
                "in_progress" => TaskStatus::InProgress,
                "completed" => TaskStatus::Completed,
                _ => return Err(format!("Invalid status: {}", status_str)),
            };
        }

        // Update other fields
        if let Some(subject) = args.get("subject").and_then(|v| v.as_str()) {
            task.subject = subject.to_string();
        }

        if let Some(description) = args.get("description").and_then(|v| v.as_str()) {
            task.description = description.to_string();
        }

        if let Some(active_form) = args.get("active_form").and_then(|v| v.as_str()) {
            task.active_form = Some(active_form.to_string());
        }

        // Handle dependencies
        if let Some(add_blocks) = args.get("add_blocks").and_then(|v| v.as_array()) {
            for block_id in add_blocks.iter().filter_map(|v| v.as_str()) {
                if !task.blocks.contains(&block_id.to_string()) {
                    task.blocks.push(block_id.to_string());
                }
            }
        }

        if let Some(add_blocked_by) = args.get("add_blocked_by").and_then(|v| v.as_array()) {
            for blocked_id in add_blocked_by.iter().filter_map(|v| v.as_str()) {
                if !task.blocked_by.contains(&blocked_id.to_string()) {
                    task.blocked_by.push(blocked_id.to_string());
                }
            }
        }

        let status = task.status.to_string();
        let subject = task.subject.clone();

        self.save_task_state(session_id, &task_state, ext_data).await?;

        Ok(vec![Content::text(format!(
            "Updated task #{}: {} [{}]",
            task_id, subject, status
        ))])
    }

    async fn handle_task_list(
        &self,
        session_id: &str,
        _arguments: Option<JsonObject>,
    ) -> Result<Vec<Content>, String> {
        let (task_state, _) = self.get_task_state(session_id).await?;
        let tasks = task_state.list_tasks();

        if tasks.is_empty() {
            return Ok(vec![Content::text("No tasks found.")]);
        }

        let mut output = String::from("Tasks:\n");
        for task in tasks {
            let status_icon = match task.status {
                TaskStatus::Pending => "○",
                TaskStatus::InProgress => "◐",
                TaskStatus::Completed => "●",
            };

            output.push_str(&format!(
                "{} #{} [{}] {}\n",
                status_icon, task.id, task.status, task.subject
            ));

            if !task.blocked_by.is_empty() {
                output.push_str(&format!("    blocked by: {}\n", task.blocked_by.join(", ")));
            }
        }

        // Summary
        let pending = tasks.iter().filter(|t| t.status == TaskStatus::Pending).count();
        let in_progress = tasks.iter().filter(|t| t.status == TaskStatus::InProgress).count();
        let completed = tasks.iter().filter(|t| t.status == TaskStatus::Completed).count();

        output.push_str(&format!(
            "\nSummary: {} pending, {} in progress, {} completed",
            pending, in_progress, completed
        ));

        Ok(vec![Content::text(output)])
    }

    async fn handle_task_get(
        &self,
        session_id: &str,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<Content>, String> {
        let args = arguments.ok_or("Missing arguments")?;

        let task_id = args
            .get("task_id")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: task_id")?;

        let (task_state, _) = self.get_task_state(session_id).await?;

        let task = task_state
            .get_task(task_id)
            .ok_or_else(|| format!("Task #{} not found", task_id))?;

        let mut output = format!(
            "Task #{}\n\
             Subject: {}\n\
             Status: {}\n\
             Description: {}\n",
            task.id, task.subject, task.status, task.description
        );

        if let Some(active_form) = &task.active_form {
            output.push_str(&format!("Active form: {}\n", active_form));
        }

        if !task.blocks.is_empty() {
            output.push_str(&format!("Blocks: {}\n", task.blocks.join(", ")));
        }

        if !task.blocked_by.is_empty() {
            output.push_str(&format!("Blocked by: {}\n", task.blocked_by.join(", ")));
        }

        Ok(vec![Content::text(output)])
    }

    // Legacy todo_write handler (kept for backward compatibility)
    async fn handle_write_todo(
        &self,
        session_id: &str,
        arguments: Option<JsonObject>,
    ) -> Result<Vec<Content>, String> {
        let content = arguments
            .as_ref()
            .ok_or("Missing arguments")?
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: content")?
            .to_string();

        let char_count = content.chars().count();
        let max_chars = std::env::var("GOOSE_TODO_MAX_CHARS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(50_000);

        if max_chars > 0 && char_count > max_chars {
            return Err(format!(
                "Todo list too large: {} chars (max: {})",
                char_count, max_chars
            ));
        }

        let manager = &self.context.session_manager;
        match manager.get_session(session_id, false).await {
            Ok(mut session) => {
                let todo_state = extension_data::TodoState::new(content);
                if todo_state
                    .to_extension_data(&mut session.extension_data)
                    .is_ok()
                {
                    match manager
                        .update(session_id)
                        .extension_data(session.extension_data)
                        .apply()
                        .await
                    {
                        Ok(_) => Ok(vec![Content::text(format!(
                            "Updated ({} chars)",
                            char_count
                        ))]),
                        Err(_) => Err("Failed to update session metadata".to_string()),
                    }
                } else {
                    Err("Failed to serialize TODO state".to_string())
                }
            }
            Err(_) => Err("Failed to read session metadata".to_string()),
        }
    }

    fn get_tools() -> Vec<Tool> {
        let mut tools = Vec::new();

        // task_create
        let schema = schema_for!(TaskCreateParams);
        let schema_value = serde_json::to_value(schema).expect("Failed to serialize schema");
        tools.push(
            Tool::new(
                "task_create".to_string(),
                indoc! {r#"
                    Create a new task for tracking work progress.

                    Use this when:
                    - Starting a complex task with multiple steps
                    - Planning implementation work
                    - Breaking down a large request into subtasks

                    The task will be created with 'pending' status.
                "#}
                .to_string(),
                schema_value.as_object().unwrap().clone(),
            )
            .annotate(ToolAnnotations {
                title: Some("Create Task".to_string()),
                read_only_hint: Some(false),
                destructive_hint: Some(false),
                idempotent_hint: Some(false),
                open_world_hint: Some(false),
            }),
        );

        // task_update
        let schema = schema_for!(TaskUpdateParams);
        let schema_value = serde_json::to_value(schema).expect("Failed to serialize schema");
        tools.push(
            Tool::new(
                "task_update".to_string(),
                indoc! {r#"
                    Update an existing task's status or details.

                    Status transitions:
                    - pending → in_progress: When starting work
                    - in_progress → completed: When work is done
                    - Any status can go back to pending if needed

                    Use add_blocked_by to set dependencies between tasks.
                "#}
                .to_string(),
                schema_value.as_object().unwrap().clone(),
            )
            .annotate(ToolAnnotations {
                title: Some("Update Task".to_string()),
                read_only_hint: Some(false),
                destructive_hint: Some(false),
                idempotent_hint: Some(true),
                open_world_hint: Some(false),
            }),
        );

        // task_list
        tools.push(
            Tool::new(
                "task_list".to_string(),
                "List all tasks with their current status. Shows pending, in_progress, and completed tasks."
                    .to_string(),
                serde_json::Map::new(),
            )
            .annotate(ToolAnnotations {
                title: Some("List Tasks".to_string()),
                read_only_hint: Some(true),
                destructive_hint: Some(false),
                idempotent_hint: Some(true),
                open_world_hint: Some(false),
            }),
        );

        // task_get
        let schema = schema_for!(TaskGetParams);
        let schema_value = serde_json::to_value(schema).expect("Failed to serialize schema");
        tools.push(
            Tool::new(
                "task_get".to_string(),
                "Get detailed information about a specific task by ID.".to_string(),
                schema_value.as_object().unwrap().clone(),
            )
            .annotate(ToolAnnotations {
                title: Some("Get Task".to_string()),
                read_only_hint: Some(true),
                destructive_hint: Some(false),
                idempotent_hint: Some(true),
                open_world_hint: Some(false),
            }),
        );

        // Legacy todo_write (kept for backward compatibility)
        let schema = schema_for!(TodoWriteParams);
        let schema_value = serde_json::to_value(schema).expect("Failed to serialize schema");
        tools.push(
            Tool::new(
                "todo_write".to_string(),
                indoc! {r#"
                    [Legacy] Overwrite the entire TODO content as plain text.

                    Prefer using task_create/task_update for structured task management.
                    This tool is kept for backward compatibility.
                "#}
                .to_string(),
                schema_value.as_object().unwrap().clone(),
            )
            .annotate(ToolAnnotations {
                title: Some("Write TODO (Legacy)".to_string()),
                read_only_hint: Some(false),
                destructive_hint: Some(true),
                idempotent_hint: Some(false),
                open_world_hint: Some(false),
            }),
        );

        tools
    }

    fn format_task_for_moim(task: &Task) -> String {
        let status_icon = match task.status {
            TaskStatus::Pending => "○",
            TaskStatus::InProgress => "◐",
            TaskStatus::Completed => "●",
        };
        format!("{} #{} {}", status_icon, task.id, task.subject)
    }
}

#[async_trait]
impl McpClientTrait for TodoClient {
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
        session_id: &str,
        name: &str,
        arguments: Option<JsonObject>,
        _working_dir: Option<&str>,
        _cancellation_token: CancellationToken,
    ) -> Result<CallToolResult, Error> {
        let content = match name {
            "task_create" => self.handle_task_create(session_id, arguments).await,
            "task_update" => self.handle_task_update(session_id, arguments).await,
            "task_list" => self.handle_task_list(session_id, arguments).await,
            "task_get" => self.handle_task_get(session_id, arguments).await,
            "todo_write" => self.handle_write_todo(session_id, arguments).await,
            _ => Err(format!("Unknown tool: {}", name)),
        };

        match content {
            Ok(content) => Ok(CallToolResult::success(content)),
            Err(error) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Error: {}",
                error
            ))])),
        }
    }

    fn get_info(&self) -> Option<&InitializeResult> {
        Some(&self.info)
    }

    async fn get_moim(&self, session_id: &str) -> Option<String> {
        let session = self
            .context
            .session_manager
            .get_session(session_id, false)
            .await
            .ok()?;

        // First, check for structured tasks
        if let Some(task_state) = TaskState::from_extension_data(&session.extension_data) {
            if !task_state.tasks.is_empty() {
                let mut output = String::from("Current tasks:\n");
                for task in task_state.list_tasks() {
                    output.push_str(&Self::format_task_for_moim(task));
                    output.push('\n');
                }
                return Some(output);
            }
        }

        // Fall back to legacy todo content
        match extension_data::TodoState::from_extension_data(&session.extension_data) {
            Some(state) if !state.content.trim().is_empty() => {
                Some(format!("Current tasks and notes:\n{}\n", state.content))
            }
            _ => Some(
                "Current tasks and notes:\nUse task_create to track complex multi-step work.\n"
                    .to_string(),
            ),
        }
    }
}
