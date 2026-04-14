//! Task notification parser for CloudCoder coordinator mode.
//!
//! This module handles parsing XML task notifications from worker subprocesses.
//! Workers output XML-formatted notifications when they complete tasks, and the
//! coordinator parses these to track worker status and results.

use quick_xml::de::from_str;
use quick_xml::se::to_string;
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

/// Error type for XML parsing failures
#[derive(Debug, Error)]
pub enum ParseError {
    /// XML is malformed or invalid
    #[error("XML parsing error: {0}")]
    XmlError(#[from] quick_xml::DeError),

    /// Required field is missing
    #[error("Missing required field: {0}")]
    MissingField(String),

    /// Invalid status value
    #[error("Invalid task status: '{0}'. Expected 'completed', 'failed', or 'killed'")]
    InvalidStatus(String),
}

/// Error type for validation failures
#[derive(Debug, Error)]
pub enum ValidationError {
    /// Task ID is empty or invalid
    #[error("Invalid task_id: {0}")]
    InvalidTaskId(String),

    /// Summary is empty
    #[error("Summary cannot be empty")]
    EmptySummary,

    /// Usage values are invalid
    #[error("Invalid usage: {0}")]
    InvalidUsage(String),
}

/// Status of a task notification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    /// Task completed successfully
    Completed,
    /// Task failed with an error
    Failed,
    /// Task was killed by the coordinator
    Killed,
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskStatus::Completed => write!(f, "completed"),
            TaskStatus::Failed => write!(f, "failed"),
            TaskStatus::Killed => write!(f, "killed"),
        }
    }
}

impl std::str::FromStr for TaskStatus {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "completed" => Ok(TaskStatus::Completed),
            "failed" => Ok(TaskStatus::Failed),
            "killed" => Ok(TaskStatus::Killed),
            _ => Err(ParseError::InvalidStatus(s.to_string())),
        }
    }
}

/// Usage statistics for a task
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskUsage {
    /// Total tokens consumed
    #[serde(rename = "total_tokens")]
    pub total_tokens: u64,

    /// Number of tool invocations
    #[serde(rename = "tool_uses")]
    pub tool_uses: u64,

    /// Task duration in milliseconds
    #[serde(rename = "duration_ms")]
    pub duration_ms: u64,
}

/// Task notification from a worker process
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename = "task-notification")]
pub struct TaskNotification {
    /// Unique identifier for the task
    #[serde(rename = "task-id")]
    pub task_id: String,

    /// Current status of the task
    pub status: TaskStatus,

    /// Human-readable summary of the task
    pub summary: String,

    /// Optional result details
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,

    /// Optional usage statistics
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<TaskUsage>,
}

/// Internal structure for XML deserialization with flexible field ordering
#[derive(Debug, Clone, Deserialize)]
struct TaskNotificationXml {
    #[serde(rename = "task-id")]
    task_id: String,
    status: String,
    summary: String,
    #[serde(default)]
    result: Option<String>,
    #[serde(default)]
    usage: Option<TaskUsageXml>,
}

#[derive(Debug, Clone, Deserialize)]
struct TaskUsageXml {
    #[serde(rename = "total_tokens")]
    total_tokens: u64,
    #[serde(rename = "tool_uses")]
    tool_uses: u64,
    #[serde(rename = "duration_ms")]
    duration_ms: u64,
}

impl From<TaskUsageXml> for TaskUsage {
    fn from(xml: TaskUsageXml) -> Self {
        TaskUsage {
            total_tokens: xml.total_tokens,
            tool_uses: xml.tool_uses,
            duration_ms: xml.duration_ms,
        }
    }
}

/// Parse an XML string into a TaskNotification.
///
/// # Arguments
///
/// * `xml` - The XML string to parse
///
/// # Returns
///
/// A `Result` containing the parsed `TaskNotification` or a `ParseError`.
///
/// # Example
///
/// ```ignore
/// let xml = r#"
///   <task-notification>
///     <task-id>agent-a1b2c3</task-id>
///     <status>completed</status>
///     <summary>Task completed</summary>
///   </task-notification>
/// "#;
/// let notification = parse(xml)?;
/// assert_eq!(notification.task_id, "agent-a1b2c3");
/// ```
pub fn parse(xml: &str) -> Result<TaskNotification, ParseError> {
    // Parse the XML structure
    let parsed: TaskNotificationXml = from_str(xml.trim())?;

    // Convert status string to enum
    let status: TaskStatus = parsed.status.parse()?;

    // Convert usage if present
    let usage = parsed.usage.map(TaskUsage::from);

    Ok(TaskNotification {
        task_id: parsed.task_id,
        status,
        summary: parsed.summary,
        result: parsed.result,
        usage,
    })
}

/// Validate a TaskNotification has all required fields with valid values.
///
/// # Arguments
///
/// * `notification` - The notification to validate
///
/// # Returns
///
/// A `Result` indicating success or a `ValidationError` if validation fails.
///
/// # Example
///
/// ```ignore
/// let notification = TaskNotification {
///     task_id: "agent-123".to_string(),
///     status: TaskStatus::Completed,
///     summary: "Done".to_string(),
///     result: None,
///     usage: None,
/// };
/// validate(&notification)?; // OK
/// ```
pub fn validate(notification: &TaskNotification) -> Result<(), ValidationError> {
    // Validate task_id is non-empty
    if notification.task_id.trim().is_empty() {
        return Err(ValidationError::InvalidTaskId(
            "task_id cannot be empty".to_string(),
        ));
    }

    // Validate summary is non-empty
    if notification.summary.trim().is_empty() {
        return Err(ValidationError::EmptySummary);
    }

    // Validate usage if present
    if let Some(ref usage) = notification.usage {
        // Check for reasonable limits (not overflowing practical values)
        if usage.total_tokens > 1_000_000_000 {
            return Err(ValidationError::InvalidUsage(
                "total_tokens exceeds reasonable limit".to_string(),
            ));
        }
        if usage.tool_uses > 1_000_000 {
            return Err(ValidationError::InvalidUsage(
                "tool_uses exceeds reasonable limit".to_string(),
            ));
        }
        // Duration can't exceed ~1 year in ms (for sanity check)
        if usage.duration_ms > 31_536_000_000 {
            return Err(ValidationError::InvalidUsage(
                "duration_ms exceeds reasonable limit".to_string(),
            ));
        }
    }

    Ok(())
}

/// Serialize a TaskNotification back to XML format.
///
/// This is useful for workers to output their notifications.
///
/// # Arguments
///
/// * `notification` - The notification to serialize
///
/// # Returns
///
/// An XML string representation of the notification.
///
/// # Example
///
/// ```ignore
/// let notification = TaskNotification {
///     task_id: "agent-123".to_string(),
///     status: TaskStatus::Completed,
///     summary: "Task done".to_string(),
///     result: Some("Found the issue".to_string()),
///     usage: None,
/// };
/// let xml = to_xml(&notification);
/// ```
pub fn to_xml(notification: &TaskNotification) -> String {
    // Use quick-xml's serialization
    match to_string(notification) {
        Ok(xml) => xml,
        Err(_) => {
            // Fallback to manual serialization for robustness
            let mut result = String::from("<task-notification>\n");
            result.push_str(&format!("  <task-id>{}</task-id>\n", escape_xml(&notification.task_id)));
            result.push_str(&format!("  <status>{}</status>\n", notification.status));
            result.push_str(&format!("  <summary>{}</summary>\n", escape_xml(&notification.summary)));

            if let Some(ref r) = notification.result {
                result.push_str(&format!("  <result>{}</result>\n", escape_xml(r)));
            }

            if let Some(ref usage) = notification.usage {
                result.push_str("  <usage>\n");
                result.push_str(&format!("    <total_tokens>{}</total_tokens>\n", usage.total_tokens));
                result.push_str(&format!("    <tool_uses>{}</tool_uses>\n", usage.tool_uses));
                result.push_str(&format!("    <duration_ms>{}</duration_ms>\n", usage.duration_ms));
                result.push_str("  </usage>\n");
            }

            result.push_str("</task-notification>");
            result
        }
    }
}

/// Escape special XML characters
fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_complete_notification() {
        let xml = r#"
            <task-notification>
                <task-id>agent-a1b2c3</task-id>
                <status>completed</status>
                <summary>Agent "Research auth bug" completed</summary>
                <result>Found null pointer in src/auth/validate.ts:42...</result>
                <usage>
                    <total_tokens>12345</total_tokens>
                    <tool_uses>23</tool_uses>
                    <duration_ms>45678</duration_ms>
                </usage>
            </task-notification>
        "#;

        let notification = parse(xml).expect("Failed to parse valid XML");

        assert_eq!(notification.task_id, "agent-a1b2c3");
        assert_eq!(notification.status, TaskStatus::Completed);
        assert_eq!(notification.summary, "Agent \"Research auth bug\" completed");
        assert!(notification.result.is_some());
        assert!(notification.usage.is_some());

        let usage = notification.usage.unwrap();
        assert_eq!(usage.total_tokens, 12345);
        assert_eq!(usage.tool_uses, 23);
        assert_eq!(usage.duration_ms, 45678);
    }

    #[test]
    fn test_parse_minimal_notification() {
        let xml = r#"
            <task-notification>
                <task-id>agent-xyz789</task-id>
                <status>failed</status>
                <summary>Task failed due to timeout</summary>
            </task-notification>
        "#;

        let notification = parse(xml).expect("Failed to parse minimal XML");

        assert_eq!(notification.task_id, "agent-xyz789");
        assert_eq!(notification.status, TaskStatus::Failed);
        assert_eq!(notification.summary, "Task failed due to timeout");
        assert!(notification.result.is_none());
        assert!(notification.usage.is_none());
    }

    #[test]
    fn test_parse_killed_status() {
        let xml = r#"
            <task-notification>
                <task-id>agent-kill123</task-id>
                <status>killed</status>
                <summary>Task was killed by coordinator</summary>
            </task-notification>
        "#;

        let notification = parse(xml).expect("Failed to parse killed status");

        assert_eq!(notification.status, TaskStatus::Killed);
    }

    #[test]
    fn test_parse_malformed_xml() {
        let xml = r#"
            <task-notification>
                <task-id>agent-123</task-id>
                <status>completed</status>
                <!-- Missing closing tag -->
        "#;

        let result = parse(xml);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_status() {
        let xml = r#"
            <task-notification>
                <task-id>agent-123</task-id>
                <status>invalid_status</status>
                <summary>Task summary</summary>
            </task-notification>
        "#;

        let result = parse(xml);
        assert!(result.is_err());
        match result {
            Err(ParseError::InvalidStatus(s)) => assert_eq!(s, "invalid_status"),
            _ => panic!("Expected InvalidStatus error"),
        }
    }

    #[test]
    fn test_validate_valid_notification() {
        let notification = TaskNotification {
            task_id: "agent-123".to_string(),
            status: TaskStatus::Completed,
            summary: "Task completed successfully".to_string(),
            result: None,
            usage: None,
        };

        assert!(validate(&notification).is_ok());
    }

    #[test]
    fn test_validate_empty_task_id() {
        let notification = TaskNotification {
            task_id: "".to_string(),
            status: TaskStatus::Completed,
            summary: "Task summary".to_string(),
            result: None,
            usage: None,
        };

        let result = validate(&notification);
        assert!(matches!(result, Err(ValidationError::InvalidTaskId(_))));
    }

    #[test]
    fn test_validate_empty_summary() {
        let notification = TaskNotification {
            task_id: "agent-123".to_string(),
            status: TaskStatus::Completed,
            summary: "".to_string(),
            result: None,
            usage: None,
        };

        let result = validate(&notification);
        assert!(matches!(result, Err(ValidationError::EmptySummary)));
    }

    #[test]
    fn test_validate_usage_limits() {
        let notification = TaskNotification {
            task_id: "agent-123".to_string(),
            status: TaskStatus::Completed,
            summary: "Task summary".to_string(),
            result: None,
            usage: Some(TaskUsage {
                total_tokens: 2_000_000_000, // Exceeds limit
                tool_uses: 5,
                duration_ms: 100,
            }),
        };

        let result = validate(&notification);
        assert!(matches!(result, Err(ValidationError::InvalidUsage(_))));
    }

    #[test]
    fn test_to_xml_with_all_fields() {
        let notification = TaskNotification {
            task_id: "agent-123".to_string(),
            status: TaskStatus::Completed,
            summary: "Task completed".to_string(),
            result: Some("Found the bug".to_string()),
            usage: Some(TaskUsage {
                total_tokens: 1000,
                tool_uses: 10,
                duration_ms: 5000,
            }),
        };

        let xml = to_xml(&notification);

        assert!(xml.contains("<task-id>agent-123</task-id>"));
        assert!(xml.contains("<status>completed</status>"));
        assert!(xml.contains("<summary>Task completed</summary>"));
        assert!(xml.contains("<result>Found the bug</result>"));
        assert!(xml.contains("<total_tokens>1000</total_tokens>"));
        assert!(xml.contains("<tool_uses>10</tool_uses>"));
        assert!(xml.contains("<duration_ms>5000</duration_ms>"));
    }

    #[test]
    fn test_to_xml_without_optional_fields() {
        let notification = TaskNotification {
            task_id: "agent-456".to_string(),
            status: TaskStatus::Failed,
            summary: "Task failed".to_string(),
            result: None,
            usage: None,
        };

        let xml = to_xml(&notification);

        assert!(xml.contains("<task-id>agent-456</task-id>"));
        assert!(xml.contains("<status>failed</status>"));
        assert!(xml.contains("<summary>Task failed</summary>"));
        assert!(!xml.contains("<result>"));
        assert!(!xml.contains("<usage>"));
    }

    #[test]
    fn test_roundtrip() {
        let original = TaskNotification {
            task_id: "agent-roundtrip".to_string(),
            status: TaskStatus::Completed,
            summary: "Round trip test".to_string(),
            result: Some("Test result".to_string()),
            usage: Some(TaskUsage {
                total_tokens: 500,
                tool_uses: 5,
                duration_ms: 3000,
            }),
        };

        let xml = to_xml(&original);
        let parsed = parse(&xml).expect("Failed to parse roundtrip XML");

        assert_eq!(original, parsed);
    }

    #[test]
    fn test_escape_xml_special_characters() {
        let xml = r#"
            <task-notification>
                <task-id>agent-special</task-id>
                <status>completed</status>
                <summary>Test with &lt;special&gt; chars &amp; "quotes"</summary>
                <result>Result with &amp; and &lt;tags&gt;</result>
            </task-notification>
        "#;

        let notification = parse(xml).expect("Failed to parse XML with special chars");
        // The XML parser should decode the entities automatically
        assert!(notification.summary.contains("&"));
        assert!(notification.summary.contains("<"));
        assert!(notification.summary.contains(">"));
    }

    #[test]
    fn test_whitespace_variations() {
        // Test that varying whitespace is handled gracefully
        let xml = r#"<task-notification>
            <task-id>agent-ws</task-id>
            <status>completed</status>
            <summary>Whitespace test</summary>
        </task-notification>"#;

        let notification = parse(xml).expect("Failed to parse with whitespace");
        assert_eq!(notification.task_id, "agent-ws");
        assert_eq!(notification.summary, "Whitespace test");
    }

    #[test]
    fn test_status_case_insensitive() {
        // Test that status parsing is case-insensitive via FromStr
        let status: TaskStatus = "COMPLETED".parse().expect("Should parse uppercase");
        assert_eq!(status, TaskStatus::Completed);

        let status: TaskStatus = "Failed".parse().expect("Should parse mixed case");
        assert_eq!(status, TaskStatus::Failed);
    }
}