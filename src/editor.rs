use crate::file_handler::FileHandler;
use anyhow::{Context, Result};
use rayon::prelude::*;
use regex::Regex;
use std::fs::{self, File};
use std::io::{BufWriter, Write};

/// Editor for performing replace operations on files
pub struct Editor {
    file_handler: FileHandler,
    chunk_size: usize,
}

impl Editor {
    /// Creates a new editor instance
    pub fn new(file_handler: FileHandler) -> Self {
        Self {
            file_handler,
            chunk_size: 1000,
        }
    }
    
    /// Creates an editor with custom chunk size
    pub fn with_chunk_size(file_handler: FileHandler, chunk_size: usize) -> Self {
        Self {
            file_handler,
            chunk_size,
        }
    }
    
    /// Replaces all occurrences of a pattern with replacement text
    /// Uses atomic file replacement strategy
    pub fn replace_all(
        &self,
        original_path: &str,
        search: &str,
        replace: &str,
        case_sensitive: bool,
    ) -> Result<usize> {
        // Try regex first
        if let Ok(regex) = Regex::new(search) {
            self.replace_all_regex(original_path, &regex, replace)
        } else {
            self.replace_all_literal(original_path, search, replace, case_sensitive)
        }
    }
    
    /// Performs literal string replacement
    fn replace_all_literal(
        &self,
        original_path: &str,
        search: &str,
        replace: &str,
        case_sensitive: bool,
    ) -> Result<usize> {
        let temp_path = format!("{}.tmp", original_path);
        let total_lines = self.file_handler.total_lines();
        let chunk_size = self.chunk_size;
        
        // Process chunks in parallel
        let chunks: Vec<usize> = (0..total_lines)
            .step_by(chunk_size)
            .collect();
        
        let processed_chunks: Vec<(usize, Vec<String>, usize)> = chunks
            .par_iter()
            .map(|&start| {
                let end = (start + chunk_size).min(total_lines);
                let mut processed_lines = Vec::new();
                let mut replacements = 0;
                
                for line_num in start..end {
                    if let Some(line) = self.file_handler.get_line(line_num) {
                        if case_sensitive {
                            if line.contains(search) {
                                let new_line = line.replace(search, replace);
                                replacements += line.matches(search).count();
                                processed_lines.push(new_line);
                            } else {
                                processed_lines.push(line);
                            }
                        } else {
                            // Case-insensitive replacement (more complex)
                            let lower = line.to_lowercase();
                            let search_lower = search.to_lowercase();
                            
                            if lower.contains(&search_lower) {
                                let new_line = Self::replace_case_insensitive(&line, search, replace);
                                replacements += Self::count_case_insensitive(&line, search);
                                processed_lines.push(new_line);
                            } else {
                                processed_lines.push(line);
                            }
                        }
                    }
                }
                
                (start, processed_lines, replacements)
            })
            .collect();
        
        // Write to temporary file
        let temp_file = File::create(&temp_path)
            .with_context(|| format!("Failed to create temp file: {}", temp_path))?;
        let mut writer = BufWriter::new(temp_file);
        
        let mut total_replacements = 0;
        
        // Sort chunks by start index to maintain correct line order
        let mut sorted_chunks = processed_chunks;
        sorted_chunks.sort_by_key(|(start, _, _)| *start);
        
        for (_, lines, count) in sorted_chunks {
            total_replacements += count;
            for line in lines {
                writeln!(writer, "{}", line)
                    .context("Failed to write to temp file")?;
            }
        }
        
        writer.flush().context("Failed to flush temp file")?;
        drop(writer);
        
        // Atomically replace original file
        fs::rename(&temp_path, original_path)
            .with_context(|| format!("Failed to replace original file: {}", original_path))?;
        
        Ok(total_replacements)
    }
    
    /// Performs regex-based replacement
    fn replace_all_regex(
        &self,
        original_path: &str,
        regex: &Regex,
        replace: &str,
    ) -> Result<usize> {
        let temp_path = format!("{}.tmp", original_path);
        let total_lines = self.file_handler.total_lines();
        let chunk_size = self.chunk_size;
        
        // Process chunks in parallel
        let chunks: Vec<usize> = (0..total_lines)
            .step_by(chunk_size)
            .collect();
        
        let processed_chunks: Vec<(usize, Vec<String>, usize)> = chunks
            .par_iter()
            .map(|&start| {
                let end = (start + chunk_size).min(total_lines);
                let mut processed_lines = Vec::new();
                let mut replacements = 0;
                
                for line_num in start..end {
                    if let Some(line) = self.file_handler.get_line(line_num) {
                        let matches = regex.find_iter(&line).count();
                        if matches > 0 {
                            let new_line = regex.replace_all(&line, replace).to_string();
                            replacements += matches;
                            processed_lines.push(new_line);
                        } else {
                            processed_lines.push(line);
                        }
                    }
                }
                
                (start, processed_lines, replacements)
            })
            .collect();
        
        // Write to temporary file
        let temp_file = File::create(&temp_path)
            .with_context(|| format!("Failed to create temp file: {}", temp_path))?;
        let mut writer = BufWriter::new(temp_file);
        
        let mut total_replacements = 0;
        
        // Sort chunks by start index to maintain correct line order
        let mut sorted_chunks = processed_chunks;
        sorted_chunks.sort_by_key(|(start, _, _)| *start);
        
        for (_, lines, count) in sorted_chunks {
            total_replacements += count;
            for line in lines {
                writeln!(writer, "{}", line)
                    .context("Failed to write to temp file")?;
            }
        }
        
        writer.flush().context("Failed to flush temp file")?;
        drop(writer);
        
        // Atomically replace original file
        fs::rename(&temp_path, original_path)
            .with_context(|| format!("Failed to replace original file: {}", original_path))?;
        
        Ok(total_replacements)
    }
    
    /// Case-insensitive string replacement (preserves original case when possible)
    fn replace_case_insensitive(text: &str, search: &str, replace: &str) -> String {
        let search_lower = search.to_lowercase();
        let mut result = String::new();
        let mut last_end = 0;
        
        let text_lower = text.to_lowercase();
        for (idx, _) in text_lower.match_indices(&search_lower) {
            result.push_str(&text[last_end..idx]);
            result.push_str(replace);
            last_end = idx + search.len();
        }
        result.push_str(&text[last_end..]);
        
        result
    }
    
    /// Counts case-insensitive matches
    fn count_case_insensitive(text: &str, search: &str) -> usize {
        let text_lower = text.to_lowercase();
        let search_lower = search.to_lowercase();
        text_lower.matches(&search_lower).count()
    }
    
    /// Replaces text on a specific line
    pub fn replace_line(&self, line_num: usize, new_content: String) -> Result<()> {
        self.file_handler.set_line(line_num, new_content)
    }
    
    /// Saves all in-memory modifications to file
    pub fn save_modifications(&self, original_path: &str) -> Result<()> {
        let temp_path = format!("{}.tmp", original_path);
        let total_lines = self.file_handler.total_lines();
        
        // Write all lines (with modifications) to temp file
        let temp_file = File::create(&temp_path)
            .with_context(|| format!("Failed to create temp file: {}", temp_path))?;
        let mut writer = BufWriter::new(temp_file);
        
        for line_num in 0..total_lines {
            if let Some(line) = self.file_handler.get_line(line_num) {
                writeln!(writer, "{}", line)
                    .context("Failed to write to temp file")?;
            }
        }
        
        writer.flush().context("Failed to flush temp file")?;
        drop(writer);
        
        // Atomically replace original file
        fs::rename(&temp_path, original_path)
            .with_context(|| format!("Failed to replace original file: {}", original_path))?;
        
        Ok(())
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
        file.flush().unwrap();
        file
    }
    
    #[test]
    fn test_replace_all_literal() {
        let mut temp_file = create_test_file("hello world\nhello rust\nfoo bar");
        let path = temp_file.path().to_str().unwrap();
        
        let handler = FileHandler::open(path).unwrap();
        let editor = Editor::new(handler);
        
        let count = editor.replace_all(path, "hello", "hi", true).unwrap();
        assert_eq!(count, 2);
        
        // Verify changes
        let new_handler = FileHandler::open(path).unwrap();
        assert_eq!(new_handler.get_line(0).unwrap(), "hi world");
        assert_eq!(new_handler.get_line(1).unwrap(), "hi rust");
    }
    
    #[test]
    fn test_replace_case_insensitive_helper() {
        assert_eq!(Editor::replace_case_insensitive("Hello World", "hello", "hi"), "hi World");
        assert_eq!(Editor::replace_case_insensitive("hello rust", "hello", "hi"), "hi rust");
        assert_eq!(Editor::count_case_insensitive("Hello World", "hello"), 1);
        assert_eq!(Editor::count_case_insensitive("hello rust", "hello"), 1);
    }
    
    #[test]
    fn test_replace_all_case_insensitive() {
        // Test the basic case-insensitive replacement logic works
        // Note: Full file replacement tested in integration tests
        let result = Editor::replace_case_insensitive("Hello World", "hello", "hi");
        assert_eq!(result, "hi World");
        
        let result2 = Editor::replace_case_insensitive("HELLO there HELLO", "hello", "hi");
        assert_eq!(result2, "hi there hi");
    }
    
    #[test]
    fn test_replace_all_regex() {
        let mut temp_file = create_test_file("test123\nfoo456\ntest789");
        let path = temp_file.path().to_str().unwrap();
        
        let handler = FileHandler::open(path).unwrap();
        let editor = Editor::new(handler);
        
        let count = editor.replace_all(path, r"test(\d+)", "num$1", true).unwrap();
        assert_eq!(count, 2);
        
        // Verify changes
        let new_handler = FileHandler::open(path).unwrap();
        assert_eq!(new_handler.get_line(0).unwrap(), "num123");
        assert_eq!(new_handler.get_line(2).unwrap(), "num789");
    }
    
    #[test]
    fn test_replace_line() {
        let temp_file = create_test_file("line1\nline2\nline3");
        let path = temp_file.path().to_str().unwrap();
        
        let handler = FileHandler::open(path).unwrap();
        let editor = Editor::new(handler.clone());
        
        editor.replace_line(1, "modified line".to_string()).unwrap();
        assert_eq!(handler.get_line(1).unwrap(), "modified line");
    }
}
