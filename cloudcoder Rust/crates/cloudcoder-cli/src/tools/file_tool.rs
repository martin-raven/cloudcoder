//! File tool for file system operations

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use cloudcoder_core::CloudCoderError;

/// File tool operation types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileOperation {
    /// Read file contents
    Read,
    /// Write content to file
    Write { content: String },
    /// Append content to file
    Append { content: String },
    /// Delete file or directory
    Delete,
    /// Create directory
    CreateDir,
    /// List directory contents
    ListDir,
    /// Check if file/directory exists
    Exists,
    /// Copy file
    Copy { destination: String },
    /// Move/rename file
    Move { destination: String },
    /// Get file metadata
    Metadata,
}

/// File tool input schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileToolInput {
    /// Path to the file or directory
    pub path: String,
    /// Operation to perform
    pub operation: FileOperation,
    /// Encoding for text files (default: utf-8)
    #[serde(default = "default_encoding")]
    pub encoding: String,
    /// Maximum file size to read (in bytes, default 10MB)
    pub max_size: Option<usize>,
}

fn default_encoding() -> String {
    "utf-8".to_string()
}

/// File tool output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileToolOutput {
    /// Whether the operation succeeded
    pub success: bool,
    /// Content (for read operations)
    pub content: Option<String>,
    /// Directory entries (for list operations)
    pub entries: Option<Vec<DirEntry>>,
    /// File metadata (for metadata operations)
    pub metadata: Option<FileMetadata>,
    /// Result message
    pub message: String,
}

/// Directory entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub is_file: bool,
    pub size: Option<u64>,
}

/// File metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    pub size: u64,
    pub is_dir: bool,
    pub is_file: bool,
    pub is_symlink: bool,
    pub readonly: bool,
    pub modified: Option<String>,
    pub created: Option<String>,
}

/// File tool for file system operations
pub struct FileTool {
    default_max_size: usize,
}

impl FileTool {
    pub fn new() -> Self {
        Self {
            default_max_size: 10 * 1024 * 1024, // 10MB
        }
    }

    pub fn name(&self) -> &str {
        "FileTool"
    }

    pub fn description(&self) -> &str {
        "Perform file system operations: read, write, delete, list, copy, move"
    }

    pub async fn execute(&self, input: FileToolInput) -> Result<FileToolOutput, CloudCoderError> {
        let path = PathBuf::from(&input.path);
        let max_size = input.max_size.unwrap_or(self.default_max_size);

        match &input.operation {
            FileOperation::Read => self.read_file(&path, max_size).await,
            FileOperation::Write { content } => self.write_file(&path, content).await,
            FileOperation::Append { content } => self.append_file(&path, content).await,
            FileOperation::Delete => self.delete_path(&path).await,
            FileOperation::CreateDir => self.create_dir(&path).await,
            FileOperation::ListDir => self.list_dir(&path).await,
            FileOperation::Exists => self.exists(&path).await,
            FileOperation::Copy { destination } => self.copy_file(&path, destination).await,
            FileOperation::Move { destination } => self.move_file(&path, destination).await,
            FileOperation::Metadata => self.get_metadata(&path).await,
        }
    }

    async fn read_file(&self, path: &Path, max_size: usize) -> Result<FileToolOutput, CloudCoderError> {
        if !path.exists() {
            return Ok(FileToolOutput {
                success: false,
                content: None,
                entries: None,
                metadata: None,
                message: format!("File not found: {}", path.display()),
            });
        }

        let metadata = tokio::fs::metadata(path)
            .await
            .map_err(|e| CloudCoderError::Io(format!("Failed to get metadata: {}", e)))?;

        if metadata.len() as usize > max_size {
            return Ok(FileToolOutput {
                success: false,
                content: None,
                entries: None,
                metadata: None,
                message: format!("File too large: {} bytes (max: {})", metadata.len(), max_size),
            });
        }

        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| CloudCoderError::Io(format!("Failed to read file: {}", e)))?;

        Ok(FileToolOutput {
            success: true,
            content: Some(content),
            entries: None,
            metadata: None,
            message: format!("Read {} bytes from {}", metadata.len(), path.display()),
        })
    }

    async fn write_file(&self, path: &Path, content: &str) -> Result<FileToolOutput, CloudCoderError> {
        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| CloudCoderError::Io(format!("Failed to create directory: {}", e)))?;
        }

        tokio::fs::write(path, content)
            .await
            .map_err(|e| CloudCoderError::Io(format!("Failed to write file: {}", e)))?;

        Ok(FileToolOutput {
            success: true,
            content: None,
            entries: None,
            metadata: None,
            message: format!("Wrote {} bytes to {}", content.len(), path.display()),
        })
    }

    async fn append_file(&self, path: &Path, content: &str) -> Result<FileToolOutput, CloudCoderError> {
        use tokio::io::AsyncWriteExt;

        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await
            .map_err(|e| CloudCoderError::Io(format!("Failed to open file: {}", e)))?;

        file.write_all(content.as_bytes())
            .await
            .map_err(|e| CloudCoderError::Io(format!("Failed to append: {}", e)))?;

        Ok(FileToolOutput {
            success: true,
            content: None,
            entries: None,
            metadata: None,
            message: format!("Appended {} bytes to {}", content.len(), path.display()),
        })
    }

    async fn delete_path(&self, path: &Path) -> Result<FileToolOutput, CloudCoderError> {
        if path.is_dir() {
            tokio::fs::remove_dir_all(path)
                .await
                .map_err(|e| CloudCoderError::Io(format!("Failed to remove directory: {}", e)))?;
        } else {
            tokio::fs::remove_file(path)
                .await
                .map_err(|e| CloudCoderError::Io(format!("Failed to remove file: {}", e)))?;
        }

        Ok(FileToolOutput {
            success: true,
            content: None,
            entries: None,
            metadata: None,
            message: format!("Deleted {}", path.display()),
        })
    }

    async fn create_dir(&self, path: &Path) -> Result<FileToolOutput, CloudCoderError> {
        tokio::fs::create_dir_all(path)
            .await
            .map_err(|e| CloudCoderError::Io(format!("Failed to create directory: {}", e)))?;

        Ok(FileToolOutput {
            success: true,
            content: None,
            entries: None,
            metadata: None,
            message: format!("Created directory {}", path.display()),
        })
    }

    async fn list_dir(&self, path: &Path) -> Result<FileToolOutput, CloudCoderError> {
        if !path.is_dir() {
            return Ok(FileToolOutput {
                success: false,
                content: None,
                entries: None,
                metadata: None,
                message: format!("Not a directory: {}", path.display()),
            });
        }

        let mut entries = Vec::new();
        let mut dir = tokio::fs::read_dir(path)
            .await
            .map_err(|e| CloudCoderError::Io(format!("Failed to read directory: {}", e)))?;

        while let Ok(Some(entry)) = dir.next_entry().await {
            let entry_path = entry.path();
            let metadata = entry.metadata().await.ok();

            entries.push(DirEntry {
                name: entry.file_name().to_string_lossy().to_string(),
                path: entry_path.display().to_string(),
                is_dir: entry_path.is_dir(),
                is_file: entry_path.is_file(),
                size: metadata.as_ref().map(|m| m.len()),
            });
        }

        // Sort entries: directories first, then alphabetical
        entries.sort_by(|a, b| {
            match (a.is_dir, b.is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            }
        });

        Ok(FileToolOutput {
            success: true,
            content: None,
            entries: Some(entries.clone()),
            metadata: None,
            message: format!("Found {} entries in {}", entries.len(), path.display()),
        })
    }

    async fn exists(&self, path: &Path) -> Result<FileToolOutput, CloudCoderError> {
        let exists = path.exists();

        Ok(FileToolOutput {
            success: true,
            content: None,
            entries: None,
            metadata: None,
            message: if exists {
                format!("{} exists", path.display())
            } else {
                format!("{} does not exist", path.display())
            },
        })
    }

    async fn copy_file(&self, source: &Path, destination: &str) -> Result<FileToolOutput, CloudCoderError> {
        let dest = PathBuf::from(destination);

        // Create parent directories if needed
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| CloudCoderError::Io(format!("Failed to create directory: {}", e)))?;
        }

        tokio::fs::copy(source, &dest)
            .await
            .map_err(|e| CloudCoderError::Io(format!("Failed to copy file: {}", e)))?;

        Ok(FileToolOutput {
            success: true,
            content: None,
            entries: None,
            metadata: None,
            message: format!("Copied {} to {}", source.display(), dest.display()),
        })
    }

    async fn move_file(&self, source: &Path, destination: &str) -> Result<FileToolOutput, CloudCoderError> {
        let dest = PathBuf::from(destination);

        // Create parent directories if needed
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| CloudCoderError::Io(format!("Failed to create directory: {}", e)))?;
        }

        tokio::fs::rename(source, &dest)
            .await
            .map_err(|e| CloudCoderError::Io(format!("Failed to move file: {}", e)))?;

        Ok(FileToolOutput {
            success: true,
            content: None,
            entries: None,
            metadata: None,
            message: format!("Moved {} to {}", source.display(), dest.display()),
        })
    }

    async fn get_metadata(&self, path: &Path) -> Result<FileToolOutput, CloudCoderError> {
        if !path.exists() {
            return Ok(FileToolOutput {
                success: false,
                content: None,
                entries: None,
                metadata: None,
                message: format!("File not found: {}", path.display()),
            });
        }

        let metadata = tokio::fs::metadata(path)
            .await
            .map_err(|e| CloudCoderError::Io(format!("Failed to get metadata: {}", e)))?;

        let modified = metadata.modified().ok().and_then(|t| {
            t.duration_since(std::time::UNIX_EPOCH).ok()
        }).map(|d| d.as_secs().to_string());

        let created = metadata.created().ok().and_then(|t| {
            t.duration_since(std::time::UNIX_EPOCH).ok()
        }).map(|d| d.as_secs().to_string());

        let file_metadata = FileMetadata {
            size: metadata.len(),
            is_dir: metadata.is_dir(),
            is_file: metadata.is_file(),
            is_symlink: metadata.file_type().is_symlink(),
            readonly: metadata.permissions().readonly(),
            modified,
            created,
        };

        Ok(FileToolOutput {
            success: true,
            content: None,
            entries: None,
            metadata: Some(file_metadata.clone()),
            message: format!("Got metadata for {}", path.display()),
        })
    }
}

impl Default for FileTool {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::tools::Tool for FileTool {
    fn name(&self) -> &str {
        "FileTool"
    }

    fn description(&self) -> &str {
        "Perform file system operations: read, write, delete, list, copy, move"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_write_and_read() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        let tool = FileTool::new();

        // Write
        let write_result = tool.execute(FileToolInput {
            path: file_path.display().to_string(),
            operation: FileOperation::Write { content: "Hello, World!".to_string() },
            encoding: default_encoding(),
            max_size: None,
        }).await.unwrap();
        assert!(write_result.success);

        // Read
        let read_result = tool.execute(FileToolInput {
            path: file_path.display().to_string(),
            operation: FileOperation::Read,
            encoding: default_encoding(),
            max_size: None,
        }).await.unwrap();
        assert!(read_result.success);
        assert_eq!(read_result.content, Some("Hello, World!".to_string()));
    }
}