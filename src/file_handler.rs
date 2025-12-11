use anyhow::{Context, Result};
use memmap2::Mmap;
use std::collections::HashMap;
use std::fs::File;
use std::sync::{Arc, RwLock};

/// Handles file operations with memory-mapped I/O
#[derive(Clone, Debug)]
pub struct FileHandler {
    /// Memory-mapped file
    mmap: Arc<Mmap>,
    /// Index of line start positions in the file
    line_offsets: Arc<Vec<usize>>,
    /// In-memory modifications (line_number -> modified_content)
    modified_lines: Arc<RwLock<HashMap<usize, String>>>,
    /// Total file size in bytes
    file_size: usize,
}

impl FileHandler {
    /// Opens a file and builds the line offset index
    pub fn open(path: &str) -> Result<Self> {
        let file = File::open(path)
            .with_context(|| format!("Failed to open file: {}", path))?;
        
        let mmap = unsafe {
            Mmap::map(&file)
                .with_context(|| format!("Failed to memory-map file: {}", path))?
        };
        
        let file_size = mmap.len();
        
        // Build line offset index
        let line_offsets = Self::build_line_index(&mmap);
        
        Ok(Self {
            mmap: Arc::new(mmap),
            line_offsets: Arc::new(line_offsets),
            modified_lines: Arc::new(RwLock::new(HashMap::new())),
            file_size,
        })
    }
    
    /// Builds an index of line start positions
    fn build_line_index(mmap: &Mmap) -> Vec<usize> {
        let mut offsets = vec![0]; // First line starts at 0
        
        for (i, &byte) in mmap.iter().enumerate() {
            if byte == b'\n' {
                offsets.push(i + 1); // Next line starts after newline
            }
        }
        
        offsets
    }
    
    /// Returns the total number of lines in the file
    pub fn total_lines(&self) -> usize {
        self.line_offsets.len()
    }
    
    /// Returns the file size in bytes
    pub fn file_size(&self) -> usize {
        self.file_size
    }
    
    /// Gets a single line by line number (0-indexed)
    pub fn get_line(&self, line_num: usize) -> Option<String> {
        // Check for modified version first
        if let Ok(modified) = self.modified_lines.read() {
            if let Some(line) = modified.get(&line_num) {
                return Some(line.clone());
            }
        }
        
        // Get from memory-mapped file
        if line_num >= self.line_offsets.len() {
            return None;
        }
        
        let start = self.line_offsets[line_num];
        let end = if line_num + 1 < self.line_offsets.len() {
            self.line_offsets[line_num + 1]
        } else {
            self.mmap.len()
        };
        
        if start >= end {
            return Some(String::new());
        }
        
        // Extract line and remove trailing newline
        let line_bytes = &self.mmap[start..end];
        let line = String::from_utf8_lossy(line_bytes).to_string();
        
        // Remove trailing \n or \r\n
        Some(line.trim_end_matches(&['\n', '\r'][..]).to_string())
    }
    
    /// Gets a range of lines (viewport rendering)
    pub fn get_viewport_lines(&self, start_line: usize, count: usize) -> Vec<String> {
        let total_lines = self.total_lines();
        let end_line = (start_line + count).min(total_lines);
        
        (start_line..end_line)
            .filter_map(|i| self.get_line(i))
            .collect()
    }
    
    /// Sets a modified line in memory (for preview or undo)
    pub fn set_line(&self, line_num: usize, content: String) -> Result<()> {
        let mut modified = self.modified_lines.write()
            .map_err(|_| anyhow::anyhow!("Failed to acquire write lock"))?;
        modified.insert(line_num, content);
        Ok(())
    }
    
    /// Clears all in-memory modifications
    pub fn clear_modifications(&self) -> Result<()> {
        let mut modified = self.modified_lines.write()
            .map_err(|_| anyhow::anyhow!("Failed to acquire write lock"))?;
        modified.clear();
        Ok(())
    }
    
    /// Gets the raw bytes for a line range (for search operations)
    pub fn get_line_bytes(&self, line_num: usize) -> Option<&[u8]> {
        if line_num >= self.line_offsets.len() {
            return None;
        }
        
        let start = self.line_offsets[line_num];
        let end = if line_num + 1 < self.line_offsets.len() {
            self.line_offsets[line_num + 1]
        } else {
            self.mmap.len()
        };
        
        if start >= end {
            return Some(&[]);
        }
        
        Some(&self.mmap[start..end])
    }
    
    /// Gets all lines as an iterator (for batch operations)
    pub fn iter_lines(&self) -> impl Iterator<Item = (usize, String)> + '_ {
        (0..self.total_lines())
            .filter_map(|i| self.get_line(i).map(|line| (i, line)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;
    
    fn create_test_file(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file
    }
    
    #[test]
    fn test_open_file() {
        let temp_file = create_test_file("line1\nline2\nline3");
        let handler = FileHandler::open(temp_file.path().to_str().unwrap()).unwrap();
        assert_eq!(handler.total_lines(), 3);
    }
    
    #[test]
    fn test_get_line() {
        let temp_file = create_test_file("first\nsecond\nthird");
        let handler = FileHandler::open(temp_file.path().to_str().unwrap()).unwrap();
        
        assert_eq!(handler.get_line(0), Some("first".to_string()));
        assert_eq!(handler.get_line(1), Some("second".to_string()));
        assert_eq!(handler.get_line(2), Some("third".to_string()));
        assert_eq!(handler.get_line(3), None);
    }
    
    #[test]
    fn test_viewport_lines() {
        let temp_file = create_test_file("1\n2\n3\n4\n5");
        let handler = FileHandler::open(temp_file.path().to_str().unwrap()).unwrap();
        
        let viewport = handler.get_viewport_lines(1, 2);
        assert_eq!(viewport, vec!["2", "3"]);
    }
    
    #[test]
    fn test_modified_lines() {
        let temp_file = create_test_file("original\nline2");
        let handler = FileHandler::open(temp_file.path().to_str().unwrap()).unwrap();
        
        handler.set_line(0, "modified".to_string()).unwrap();
        assert_eq!(handler.get_line(0), Some("modified".to_string()));
        assert_eq!(handler.get_line(1), Some("line2".to_string()));
        
        handler.clear_modifications().unwrap();
        assert_eq!(handler.get_line(0), Some("original".to_string()));
    }
    
    #[test]
    fn test_empty_file() {
        let temp_file = create_test_file("");
        let handler = FileHandler::open(temp_file.path().to_str().unwrap()).unwrap();
        assert_eq!(handler.total_lines(), 1); // Empty file has one empty line
    }
    
    #[test]
    fn test_no_trailing_newline() {
        let temp_file = create_test_file("line1\nline2");
        let handler = FileHandler::open(temp_file.path().to_str().unwrap()).unwrap();
        assert_eq!(handler.total_lines(), 2);
        assert_eq!(handler.get_line(1), Some("line2".to_string()));
    }
}
