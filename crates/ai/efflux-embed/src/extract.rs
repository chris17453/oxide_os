//! Content extraction from various file types

use alloc::string::String;
use alloc::vec::Vec;
use crate::{EmbedResult, EmbedError};

/// Detected file type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    /// Plain text
    PlainText,
    /// Markdown
    Markdown,
    /// Source code
    SourceCode,
    /// PDF document
    Pdf,
    /// Image (JPEG, PNG, etc.)
    Image,
    /// Audio file
    Audio,
    /// Video file
    Video,
    /// HTML document
    Html,
    /// XML document
    Xml,
    /// JSON document
    Json,
    /// Unknown/binary
    Unknown,
}

impl FileType {
    /// Detect file type from extension
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "txt" | "text" => FileType::PlainText,
            "md" | "markdown" => FileType::Markdown,
            "rs" | "c" | "cpp" | "h" | "py" | "js" | "ts" | "go" | "java" => FileType::SourceCode,
            "pdf" => FileType::Pdf,
            "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" => FileType::Image,
            "mp3" | "wav" | "flac" | "ogg" | "m4a" => FileType::Audio,
            "mp4" | "avi" | "mkv" | "webm" | "mov" => FileType::Video,
            "html" | "htm" => FileType::Html,
            "xml" => FileType::Xml,
            "json" => FileType::Json,
            _ => FileType::Unknown,
        }
    }

    /// Detect file type from magic bytes
    pub fn from_magic(data: &[u8]) -> Self {
        if data.len() < 4 {
            return FileType::Unknown;
        }

        // Check magic bytes
        if data.starts_with(b"%PDF") {
            return FileType::Pdf;
        }
        if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
            return FileType::Image; // JPEG
        }
        if data.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
            return FileType::Image; // PNG
        }
        if data.starts_with(b"GIF8") {
            return FileType::Image; // GIF
        }
        if data.starts_with(b"RIFF") && data.len() > 8 && data[8..12] == *b"WAVE" {
            return FileType::Audio; // WAV
        }
        if data.starts_with(&[0x49, 0x44, 0x33]) || data.starts_with(&[0xFF, 0xFB]) {
            return FileType::Audio; // MP3
        }
        if data.len() > 4 && data[4..8] == *b"ftyp" {
            return FileType::Video; // MP4/MOV
        }

        // Check if it looks like text
        let is_text = data.iter().take(1024).all(|&b| {
            b.is_ascii_graphic() || b.is_ascii_whitespace() || b > 127
        });

        if is_text {
            if data.starts_with(b"<!DOCTYPE") || data.starts_with(b"<html") {
                return FileType::Html;
            }
            if data.starts_with(b"<?xml") || data.starts_with(b"<") {
                return FileType::Xml;
            }
            if data.starts_with(b"{") || data.starts_with(b"[") {
                return FileType::Json;
            }
            return FileType::PlainText;
        }

        FileType::Unknown
    }

    /// Check if file type can be indexed as text
    pub fn is_indexable(&self) -> bool {
        matches!(self,
            FileType::PlainText | FileType::Markdown | FileType::SourceCode |
            FileType::Html | FileType::Xml | FileType::Json
        )
    }
}

/// Extracted content from a file
#[derive(Debug, Clone)]
pub struct ExtractedContent {
    /// Main text content
    pub text: String,
    /// Title (if available)
    pub title: Option<String>,
    /// Summary/description
    pub summary: Option<String>,
    /// Detected language
    pub language: Option<String>,
    /// Tags/keywords
    pub tags: Vec<String>,
    /// Original file type
    pub file_type: FileType,
}

impl ExtractedContent {
    /// Create empty content
    pub fn empty(file_type: FileType) -> Self {
        ExtractedContent {
            text: String::new(),
            title: None,
            summary: None,
            language: None,
            tags: Vec::new(),
            file_type,
        }
    }
}

/// Content extractor
pub struct ContentExtractor;

impl ContentExtractor {
    /// Extract content from file data
    pub fn extract(data: &[u8], file_type: FileType) -> EmbedResult<ExtractedContent> {
        match file_type {
            FileType::PlainText | FileType::Markdown | FileType::SourceCode => {
                Self::extract_text(data, file_type)
            }
            FileType::Html => Self::extract_html(data),
            FileType::Xml => Self::extract_xml(data),
            FileType::Json => Self::extract_json(data),
            FileType::Image => Self::extract_image_metadata(data),
            FileType::Pdf => Err(EmbedError::UnsupportedType), // Requires external lib
            _ => Err(EmbedError::UnsupportedType),
        }
    }

    /// Extract plain text content
    fn extract_text(data: &[u8], file_type: FileType) -> EmbedResult<ExtractedContent> {
        let text = core::str::from_utf8(data)
            .map_err(|_| EmbedError::FileReadError)?;

        Ok(ExtractedContent {
            text: String::from(text),
            title: Self::extract_first_line(text),
            summary: Self::extract_summary(text),
            language: None,
            tags: Vec::new(),
            file_type,
        })
    }

    /// Extract from HTML (basic, strips tags)
    fn extract_html(data: &[u8]) -> EmbedResult<ExtractedContent> {
        let text = core::str::from_utf8(data)
            .map_err(|_| EmbedError::FileReadError)?;

        // Very basic HTML stripping
        let mut result = String::new();
        let mut in_tag = false;
        let mut in_script = false;
        let mut in_style = false;

        for ch in text.chars() {
            if ch == '<' {
                in_tag = true;
                continue;
            }
            if ch == '>' {
                in_tag = false;
                continue;
            }
            if in_tag {
                // Check for script/style start
                continue;
            }
            if !in_script && !in_style {
                result.push(ch);
            }
        }

        // Normalize whitespace
        let text: String = result.split_whitespace().collect::<Vec<_>>().join(" ");

        Ok(ExtractedContent {
            text,
            title: None,
            summary: None,
            language: None,
            tags: Vec::new(),
            file_type: FileType::Html,
        })
    }

    /// Extract from XML (strips tags)
    fn extract_xml(data: &[u8]) -> EmbedResult<ExtractedContent> {
        // Use same logic as HTML for basic extraction
        Self::extract_html(data).map(|mut c| {
            c.file_type = FileType::Xml;
            c
        })
    }

    /// Extract from JSON (extracts string values)
    fn extract_json(data: &[u8]) -> EmbedResult<ExtractedContent> {
        let text = core::str::from_utf8(data)
            .map_err(|_| EmbedError::FileReadError)?;

        // Very basic JSON string extraction
        let mut result = Vec::new();
        let mut in_string = false;
        let mut current = String::new();
        let mut escape = false;

        for ch in text.chars() {
            if escape {
                current.push(ch);
                escape = false;
                continue;
            }
            if ch == '\\' {
                escape = true;
                continue;
            }
            if ch == '"' {
                if in_string {
                    if !current.is_empty() {
                        result.push(current.clone());
                    }
                    current.clear();
                }
                in_string = !in_string;
                continue;
            }
            if in_string {
                current.push(ch);
            }
        }

        let text = result.join(" ");

        Ok(ExtractedContent {
            text,
            title: None,
            summary: None,
            language: None,
            tags: Vec::new(),
            file_type: FileType::Json,
        })
    }

    /// Extract image metadata (EXIF, etc.)
    fn extract_image_metadata(_data: &[u8]) -> EmbedResult<ExtractedContent> {
        // Basic stub - real implementation would parse EXIF
        Ok(ExtractedContent {
            text: String::new(),
            title: None,
            summary: None,
            language: None,
            tags: Vec::new(),
            file_type: FileType::Image,
        })
    }

    /// Extract first non-empty line as title
    fn extract_first_line(text: &str) -> Option<String> {
        text.lines()
            .find(|l| !l.trim().is_empty())
            .map(|l| l.trim().into())
    }

    /// Extract summary (first few sentences)
    fn extract_summary(text: &str) -> Option<String> {
        let words: Vec<_> = text.split_whitespace().take(50).collect();
        if words.is_empty() {
            None
        } else {
            Some(words.join(" "))
        }
    }
}
