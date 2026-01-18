//! Path handling utilities

use alloc::string::String;
use alloc::vec::Vec;

/// A filesystem path
#[derive(Debug, Clone)]
pub struct Path {
    /// Path components
    components: Vec<String>,
    /// Is this an absolute path?
    absolute: bool,
}

impl Path {
    /// Maximum path length
    pub const MAX_PATH: usize = 4096;
    /// Maximum component length
    pub const MAX_NAME: usize = 255;

    /// Create a new path from a string
    pub fn new(path: &str) -> Self {
        let absolute = path.starts_with('/');
        let components: Vec<String> = path
            .split('/')
            .filter(|s| !s.is_empty() && *s != ".")
            .map(|s| {
                if s == ".." {
                    String::from("..")
                } else {
                    String::from(s)
                }
            })
            .collect();

        Path { components, absolute }
    }

    /// Is this an absolute path?
    pub fn is_absolute(&self) -> bool {
        self.absolute
    }

    /// Is this a relative path?
    pub fn is_relative(&self) -> bool {
        !self.absolute
    }

    /// Get path components
    pub fn components(&self) -> &[String] {
        &self.components
    }

    /// Get the filename (last component)
    pub fn filename(&self) -> Option<&str> {
        self.components.last().map(|s| s.as_str())
    }

    /// Get the parent path
    pub fn parent(&self) -> Option<Path> {
        if self.components.is_empty() {
            return None;
        }

        let mut components = self.components.clone();
        components.pop();

        Some(Path {
            components,
            absolute: self.absolute,
        })
    }

    /// Join with another path
    pub fn join(&self, other: &Path) -> Path {
        if other.is_absolute() {
            return other.clone();
        }

        let mut components = self.components.clone();
        components.extend(other.components.iter().cloned());

        Path {
            components,
            absolute: self.absolute,
        }
    }

    /// Normalize the path (resolve . and ..)
    pub fn normalize(&self) -> Path {
        let mut normalized: Vec<String> = Vec::new();

        for component in &self.components {
            if component == ".." {
                if !normalized.is_empty() && normalized.last() != Some(&String::from("..")) {
                    normalized.pop();
                } else if !self.absolute {
                    normalized.push(component.clone());
                }
            } else {
                normalized.push(component.clone());
            }
        }

        Path {
            components: normalized,
            absolute: self.absolute,
        }
    }

    /// Convert to string
    pub fn to_string(&self) -> String {
        if self.components.is_empty() {
            if self.absolute {
                return String::from("/");
            } else {
                return String::from(".");
            }
        }

        let mut s = String::new();
        if self.absolute {
            s.push('/');
        }
        s.push_str(&self.components.join("/"));
        s
    }

    /// Check if path is empty
    pub fn is_empty(&self) -> bool {
        self.components.is_empty()
    }

    /// Get number of components
    pub fn len(&self) -> usize {
        self.components.len()
    }
}

impl From<&str> for Path {
    fn from(s: &str) -> Self {
        Path::new(s)
    }
}

/// Split a path into directory and filename
pub fn split_path(path: &str) -> (Option<&str>, &str) {
    if let Some(pos) = path.rfind('/') {
        if pos == 0 {
            (Some("/"), &path[1..])
        } else {
            (Some(&path[..pos]), &path[pos + 1..])
        }
    } else {
        (None, path)
    }
}

/// Check if a name is valid
pub fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= Path::MAX_NAME
        && !name.contains('/')
        && !name.contains('\0')
        && name != "."
        && name != ".."
}
