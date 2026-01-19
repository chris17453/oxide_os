//! File I/O module for GW-BASIC

use crate::error::{Error, Result};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;

/// File access modes
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileMode {
    Input,
    Output,
    Append,
    Random,
}

/// File handle information
pub struct FileHandle {
    _file: Option<File>,
    _mode: FileMode,
    _path: PathBuf,
    reader: Option<BufReader<File>>,
    writer: Option<BufWriter<File>>,
}

/// File manager
pub struct FileManager {
    handles: HashMap<i32, FileHandle>,
}

impl FileManager {
    pub fn new() -> Self {
        FileManager {
            handles: HashMap::new(),
        }
    }

    pub fn open(&mut self, file_num: i32, path: &str, mode: FileMode) -> Result<()> {
        if self.handles.contains_key(&file_num) {
            return Err(Error::RuntimeError(format!(
                "File #{} is already open",
                file_num
            )));
        }

        let file = match mode {
            FileMode::Input => File::open(path)
                .map_err(|e| Error::IoError(format!("Cannot open file: {}", e)))?,
            FileMode::Output => File::create(path)
                .map_err(|e| Error::IoError(format!("Cannot create file: {}", e)))?,
            FileMode::Append => OpenOptions::new()
                .append(true)
                .create(true)
                .open(path)
                .map_err(|e| Error::IoError(format!("Cannot open file for append: {}", e)))?,
            FileMode::Random => OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open(path)
                .map_err(|e| Error::IoError(format!("Cannot open random file: {}", e)))?,
        };

        let reader = if mode == FileMode::Input {
            Some(BufReader::new(
                File::open(path)
                    .map_err(|e| Error::IoError(format!("Cannot open file: {}", e)))?,
            ))
        } else {
            None
        };

        let writer = if mode == FileMode::Output || mode == FileMode::Append {
            Some(BufWriter::new(
                if mode == FileMode::Output {
                    File::create(path)
                } else {
                    OpenOptions::new().append(true).open(path)
                }
                .map_err(|e| Error::IoError(format!("Cannot open file for writing: {}", e)))?,
            ))
        } else {
            None
        };

        self.handles.insert(
            file_num,
            FileHandle {
                _file: Some(file),
                _mode: mode,
                _path: PathBuf::from(path),
                reader,
                writer,
            },
        );

        Ok(())
    }

    pub fn close(&mut self, file_num: i32) -> Result<()> {
        if let Some(mut handle) = self.handles.remove(&file_num) {
            if let Some(ref mut writer) = handle.writer {
                writer
                    .flush()
                    .map_err(|e| Error::IoError(format!("Error flushing file: {}", e)))?;
            }
            Ok(())
        } else {
            Err(Error::RuntimeError(format!(
                "File #{} is not open",
                file_num
            )))
        }
    }

    pub fn close_all(&mut self) -> Result<()> {
        let file_nums: Vec<i32> = self.handles.keys().copied().collect();
        for num in file_nums {
            self.close(num)?;
        }
        Ok(())
    }

    pub fn write_line(&mut self, file_num: i32, data: &str) -> Result<()> {
        if let Some(handle) = self.handles.get_mut(&file_num) {
            if let Some(ref mut writer) = handle.writer {
                writeln!(writer, "{}", data)
                    .map_err(|e| Error::IoError(format!("Error writing to file: {}", e)))?;
                Ok(())
            } else {
                Err(Error::RuntimeError(format!(
                    "File #{} not open for writing",
                    file_num
                )))
            }
        } else {
            Err(Error::RuntimeError(format!(
                "File #{} is not open",
                file_num
            )))
        }
    }

    pub fn read_line(&mut self, file_num: i32) -> Result<String> {
        if let Some(handle) = self.handles.get_mut(&file_num) {
            if let Some(ref mut reader) = handle.reader {
                let mut line = String::new();
                reader
                    .read_line(&mut line)
                    .map_err(|e| Error::IoError(format!("Error reading from file: {}", e)))?;
                Ok(line.trim_end().to_string())
            } else {
                Err(Error::RuntimeError(format!(
                    "File #{} not open for reading",
                    file_num
                )))
            }
        } else {
            Err(Error::RuntimeError(format!(
                "File #{} is not open",
                file_num
            )))
        }
    }

    pub fn eof(&self, file_num: i32) -> Result<bool> {
        if let Some(_handle) = self.handles.get(&file_num) {
            // Simplified: would need to track EOF state properly
            Ok(false)
        } else {
            Err(Error::RuntimeError(format!(
                "File #{} is not open",
                file_num
            )))
        }
    }

    pub fn loc(&self, file_num: i32) -> Result<i32> {
        if let Some(_handle) = self.handles.get(&file_num) {
            // Return current position (simulated)
            Ok(0)
        } else {
            Err(Error::RuntimeError(format!(
                "File #{} is not open",
                file_num
            )))
        }
    }

    pub fn lof(&self, file_num: i32) -> Result<i32> {
        if let Some(_handle) = self.handles.get(&file_num) {
            // Return file length (simulated)
            Ok(0)
        } else {
            Err(Error::RuntimeError(format!(
                "File #{} is not open",
                file_num
            )))
        }
    }
}

impl Default for FileManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_manager_creation() {
        let fm = FileManager::new();
        assert_eq!(fm.handles.len(), 0);
    }
}