//! AskUserQuestion tool - Structured user prompting for LLM agents
//!
//! Allows the LLM to ask users structured questions with predefined options,
//! useful for clarifying requirements, selecting between alternatives, or
//! gathering user preferences during execution.

use rmcp::model::{Content, ErrorCode, ErrorData};
use serde::{Deserialize, Serialize};
use rmcp::schemars::JsonSchema;

/// A single option for a question
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct QuestionOption {
    /// Short display label (1-5 words)
    pub label: String,

    /// Description of what this option means
    pub description: String,
}

/// A single question to ask the user
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Question {
    /// The complete question to ask
    pub question: String,

    /// Short header/label for the question (max 12 chars)
    pub header: String,

    /// Available options (2-4 options)
    pub options: Vec<QuestionOption>,

    /// Whether multiple options can be selected
    #[serde(default)]
    pub multi_select: bool,
}

/// Parameters for the ask_user_question tool
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AskUserQuestionParams {
    /// Questions to ask the user (1-4 questions)
    pub questions: Vec<Question>,
}

/// Result containing user's answers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserAnswer {
    /// The question that was asked
    pub question: String,

    /// Selected option labels
    pub selected: Vec<String>,

    /// Custom text if user chose "Other"
    pub custom_text: Option<String>,
}

/// Validates the parameters for ask_user_question
fn validate_params(params: &AskUserQuestionParams) -> Result<(), ErrorData> {
    // Check question count (1-4)
    if params.questions.is_empty() {
        return Err(ErrorData::new(
            ErrorCode::INVALID_PARAMS,
            "At least one question is required",
            None,
        ));
    }

    if params.questions.len() > 4 {
        return Err(ErrorData::new(
            ErrorCode::INVALID_PARAMS,
            "Maximum 4 questions allowed",
            None,
        ));
    }

    for (i, q) in params.questions.iter().enumerate() {
        // Check header length
        if q.header.chars().count() > 12 {
            return Err(ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                format!("Question {} header must be 12 characters or less", i + 1),
                None,
            ));
        }

        // Check options count (2-4)
        if q.options.len() < 2 {
            return Err(ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                format!("Question {} must have at least 2 options", i + 1),
                None,
            ));
        }

        if q.options.len() > 4 {
            return Err(ErrorData::new(
                ErrorCode::INVALID_PARAMS,
                format!("Question {} can have at most 4 options", i + 1),
                None,
            ));
        }
    }

    Ok(())
}

/// Formats questions for display to the user
fn format_questions_for_display(params: &AskUserQuestionParams) -> String {
    let mut output = String::new();
    output.push_str("🤔 **User Input Required**\n\n");

    for (i, q) in params.questions.iter().enumerate() {
        output.push_str(&format!("### {}. {} - {}\n\n", i + 1, q.header, q.question));

        for opt in q.options.iter() {
            let marker = if q.multi_select { "☐" } else { "○" };
            output.push_str(&format!("  {} **{}**: {}\n", marker, opt.label, opt.description));
        }

        // Always include "Other" option
        let other_marker = if q.multi_select { "☐" } else { "○" };
        output.push_str(&format!("  {} **Other**: (custom response)\n", other_marker));
        output.push('\n');
    }

    if params.questions.iter().any(|q| q.multi_select) {
        output.push_str("_For multi-select questions, you can choose multiple options._\n\n");
    }

    output.push_str("Please respond with your selections.\n");

    output
}

/// Executes the ask_user_question tool
///
/// This tool returns a structured question prompt that the agent/UI should
/// intercept and present to the user. The response format allows the agent
/// system to detect this as a user interaction request.
pub async fn ask_user_question(params: AskUserQuestionParams) -> Result<Vec<Content>, ErrorData> {
    // Validate parameters
    validate_params(&params)?;

    // Create structured response that the agent can detect
    let display_text = format_questions_for_display(&params);

    // Serialize the params for the agent to parse
    let params_json = serde_json::to_string_pretty(&params).map_err(|e| {
        ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            format!("Failed to serialize params: {}", e),
            None,
        )
    })?;

    // Return both human-readable and machine-readable content
    // The special marker allows the agent system to detect this as an interactive request
    let marker = "<!--ASK_USER_QUESTION-->";

    Ok(vec![
        Content::text(format!("{}\n\n{}\n\n```json\n{}\n```", marker, display_text, params_json)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_params_valid() {
        let params = AskUserQuestionParams {
            questions: vec![
                Question {
                    question: "Which library should we use?".to_string(),
                    header: "Library".to_string(),
                    options: vec![
                        QuestionOption {
                            label: "React".to_string(),
                            description: "Most popular UI library".to_string(),
                        },
                        QuestionOption {
                            label: "Vue".to_string(),
                            description: "Progressive framework".to_string(),
                        },
                    ],
                    multi_select: false,
                },
            ],
        };

        assert!(validate_params(&params).is_ok());
    }

    #[test]
    fn test_validate_params_empty() {
        let params = AskUserQuestionParams {
            questions: vec![],
        };

        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_validate_params_too_many_questions() {
        let make_question = || Question {
            question: "Test?".to_string(),
            header: "Test".to_string(),
            options: vec![
                QuestionOption { label: "A".to_string(), description: "A".to_string() },
                QuestionOption { label: "B".to_string(), description: "B".to_string() },
            ],
            multi_select: false,
        };

        let params = AskUserQuestionParams {
            questions: vec![make_question(), make_question(), make_question(), make_question(), make_question()],
        };

        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_validate_params_header_too_long() {
        let params = AskUserQuestionParams {
            questions: vec![
                Question {
                    question: "Test?".to_string(),
                    header: "This header is way too long".to_string(),
                    options: vec![
                        QuestionOption { label: "A".to_string(), description: "A".to_string() },
                        QuestionOption { label: "B".to_string(), description: "B".to_string() },
                    ],
                    multi_select: false,
                },
            ],
        };

        assert!(validate_params(&params).is_err());
    }

    #[test]
    fn test_format_questions() {
        let params = AskUserQuestionParams {
            questions: vec![
                Question {
                    question: "Which approach?".to_string(),
                    header: "Approach".to_string(),
                    options: vec![
                        QuestionOption {
                            label: "Option A".to_string(),
                            description: "First approach".to_string(),
                        },
                        QuestionOption {
                            label: "Option B".to_string(),
                            description: "Second approach".to_string(),
                        },
                    ],
                    multi_select: false,
                },
            ],
        };

        let output = format_questions_for_display(&params);
        assert!(output.contains("Which approach?"));
        assert!(output.contains("Option A"));
        assert!(output.contains("Option B"));
        assert!(output.contains("Other"));
    }
}
