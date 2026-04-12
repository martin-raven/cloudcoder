use std::fmt;
use std::error::Error;

/// Main error type for Cloud Coder operations
#[derive(Debug)]
pub enum CloudCoderError {
    ToolExecution {
        message: String,
        tool_name: String,
        tool_input: Option<String>,
    },
    PermissionDenied {
        tool_name: String,
        reason: Option<String>,
    },
    Api(String),
    Io(String),
    Cache(String),
    Config(String),
    Service(ServiceError),
}

impl fmt::Display for CloudCoderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CloudCoderError::ToolExecution { message, tool_name, tool_input } => {
                write!(f, "Tool execution error for '{}': {}", tool_name, message)?;
                if let Some(input) = tool_input {
                    write!(f, " (input: {})", input)?;
                }
                Ok(())
            }
            CloudCoderError::PermissionDenied { tool_name, reason } => {
                write!(f, "Permission denied for tool '{}'", tool_name)?;
                if let Some(r) = reason {
                    write!(f, ": {}", r)?;
                }
                Ok(())
            }
            CloudCoderError::Api(msg) => write!(f, "API error: {}", msg),
            CloudCoderError::Io(msg) => write!(f, "IO error: {}", msg),
            CloudCoderError::Cache(msg) => write!(f, "Cache error: {}", msg),
            CloudCoderError::Config(msg) => write!(f, "Configuration error: {}", msg),
            CloudCoderError::Service(err) => write!(f, "Service error: {}", err),
        }
    }
}

impl Error for CloudCoderError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            CloudCoderError::Service(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for CloudCoderError {
    fn from(err: std::io::Error) -> Self {
        CloudCoderError::Io(err.to_string())
    }
}

impl Clone for CloudCoderError {
    fn clone(&self) -> Self {
        match self {
            CloudCoderError::ToolExecution { message, tool_name, tool_input } => {
                CloudCoderError::ToolExecution {
                    message: message.clone(),
                    tool_name: tool_name.clone(),
                    tool_input: tool_input.clone(),
                }
            }
            CloudCoderError::PermissionDenied { tool_name, reason } => {
                CloudCoderError::PermissionDenied {
                    tool_name: tool_name.clone(),
                    reason: reason.clone(),
                }
            }
            CloudCoderError::Api(msg) => CloudCoderError::Api(msg.clone()),
            CloudCoderError::Io(msg) => CloudCoderError::Io(msg.clone()),
            CloudCoderError::Cache(msg) => CloudCoderError::Cache(msg.clone()),
            CloudCoderError::Config(msg) => CloudCoderError::Config(msg.clone()),
            CloudCoderError::Service(err) => CloudCoderError::Service(err.clone()),
        }
    }
}

impl From<ServiceError> for CloudCoderError {
    fn from(err: ServiceError) -> Self {
        CloudCoderError::Service(err)
    }
}

/// Error type for service operations
#[derive(Debug)]
pub struct ServiceError {
    pub message: String,
    pub source: Option<Box<dyn Error + Send + Sync>>,
}

impl fmt::Display for ServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(src) = &self.source {
            write!(f, " (caused by: {})", src)?;
        }
        Ok(())
    }
}

impl Error for ServiceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_ref().map(|b| b.as_ref() as &(dyn Error + 'static))
    }
}

impl ServiceError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            source: None,
        }
    }

    pub fn with_source(message: impl Into<String>, source: impl Error + Send + Sync + 'static) -> Self {
        Self {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }
}

impl Clone for ServiceError {
    fn clone(&self) -> Self {
        Self {
            message: self.message.clone(),
            source: None, // We can't clone the source, so we drop it
        }
    }
}