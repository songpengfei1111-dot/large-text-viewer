use crate::file_handler::FileHandler;
use anyhow::Result;
use rayon::prelude::*;
use regex::Regex;

/// Search result containing line number and matched content
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub line_number: usize,
    pub line_content: String,
    pub match_start: usize,
    pub match_end: usize,
}

/// Performs parallel search across the file
pub struct SearchEngine {
    file_handler: FileHandler,
    chunk_size: usize,
}

impl SearchEngine {
    /// Creates a new search engine with default chunk size
    pub fn new(file_handler: FileHandler) -> Self {
        Self {
            file_handler,
            chunk_size: 1000, // Process 1000 lines per chunk
        }
    }
    
    /// Creates a search engine with custom chunk size
    pub fn with_chunk_size(file_handler: FileHandler, chunk_size: usize) -> Self {
        Self {
            file_handler,
            chunk_size,
        }
    }
    
    /// Searches for a query string (auto-detects regex vs literal)
    pub fn search(&self, query: &str, case_sensitive: bool) -> Result<Vec<SearchResult>> {
        // Try to compile as regex
        if let Ok(regex) = Regex::new(query) {
            self.search_regex(&regex)
        } else {
            self.search_literal(query, case_sensitive)
        }
    }
    
    /// Performs literal string search
    pub fn search_literal(&self, query: &str, case_sensitive: bool) -> Result<Vec<SearchResult>> {
        let total_lines = self.file_handler.total_lines();
        let chunk_size = self.chunk_size;
        
        // Create chunks
        let chunks: Vec<usize> = (0..total_lines)
            .step_by(chunk_size)
            .collect();
        
        let query_lower = if !case_sensitive {
            query.to_lowercase()
        } else {
            query.to_string()
        };
        
        // Search in parallel
        let results: Vec<Vec<SearchResult>> = chunks
            .par_iter()
            .map(|&start| {
                let end = (start + chunk_size).min(total_lines);
                let mut chunk_results = Vec::new();
                
                for line_num in start..end {
                    if let Some(line) = self.file_handler.get_line(line_num) {
                        let search_line = if !case_sensitive {
                            line.to_lowercase()
                        } else {
                            line.clone()
                        };
                        
                        if let Some(pos) = search_line.find(&query_lower) {
                            chunk_results.push(SearchResult {
                                line_number: line_num,
                                line_content: line,
                                match_start: pos,
                                match_end: pos + query.len(),
                            });
                        }
                    }
                }
                
                chunk_results
            })
            .collect();
        
        // Flatten results
        Ok(results.into_iter().flatten().collect())
    }
    
    /// Performs regex search
    pub fn search_regex(&self, regex: &Regex) -> Result<Vec<SearchResult>> {
        let total_lines = self.file_handler.total_lines();
        let chunk_size = self.chunk_size;
        
        // Create chunks
        let chunks: Vec<usize> = (0..total_lines)
            .step_by(chunk_size)
            .collect();
        
        // Search in parallel
        let results: Vec<Vec<SearchResult>> = chunks
            .par_iter()
            .map(|&start| {
                let end = (start + chunk_size).min(total_lines);
                let mut chunk_results = Vec::new();
                
                for line_num in start..end {
                    if let Some(line) = self.file_handler.get_line(line_num) {
                        if let Some(mat) = regex.find(&line) {
                            chunk_results.push(SearchResult {
                                line_number: line_num,
                                line_content: line.clone(),
                                match_start: mat.start(),
                                match_end: mat.end(),
                            });
                        }
                    }
                }
                
                chunk_results
            })
            .collect();
        
        // Flatten results
        Ok(results.into_iter().flatten().collect())
    }
    
    /// Finds the next match after a given line number
    pub fn find_next(&self, query: &str, from_line: usize, case_sensitive: bool) -> Option<SearchResult> {
        let total_lines = self.file_handler.total_lines();
        
        let query_lower = if !case_sensitive {
            query.to_lowercase()
        } else {
            query.to_string()
        };
        
        for line_num in (from_line + 1)..total_lines {
            if let Some(line) = self.file_handler.get_line(line_num) {
                let search_line = if !case_sensitive {
                    line.to_lowercase()
                } else {
                    line.clone()
                };
                
                if let Some(pos) = search_line.find(&query_lower) {
                    return Some(SearchResult {
                        line_number: line_num,
                        line_content: line,
                        match_start: pos,
                        match_end: pos + query.len(),
                    });
                }
            }
        }
        
        None
    }
    
    /// Finds the previous match before a given line number
    pub fn find_previous(&self, query: &str, from_line: usize, case_sensitive: bool) -> Option<SearchResult> {
        let query_lower = if !case_sensitive {
            query.to_lowercase()
        } else {
            query.to_string()
        };
        
        for line_num in (0..from_line).rev() {
            if let Some(line) = self.file_handler.get_line(line_num) {
                let search_line = if !case_sensitive {
                    line.to_lowercase()
                } else {
                    line.clone()
                };
                
                if let Some(pos) = search_line.find(&query_lower) {
                    return Some(SearchResult {
                        line_number: line_num,
                        line_content: line,
                        match_start: pos,
                        match_end: pos + query.len(),
                    });
                }
            }
        }
        
        None
    }
    
    /// Counts total matches without collecting all results
    pub fn count_matches(&self, query: &str, case_sensitive: bool) -> Result<usize> {
        let results = self.search_literal(query, case_sensitive)?;
        Ok(results.len())
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
    fn test_literal_search() {
        let temp_file = create_test_file("hello world\nfoo bar\nhello again");
        let handler = FileHandler::open(temp_file.path().to_str().unwrap()).unwrap();
        let searcher = SearchEngine::new(handler);
        
        let results = searcher.search_literal("hello", true).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].line_number, 0);
        assert_eq!(results[1].line_number, 2);
    }
    
    #[test]
    fn test_case_insensitive_search() {
        let temp_file = create_test_file("Hello World\nfoo bar");
        let handler = FileHandler::open(temp_file.path().to_str().unwrap()).unwrap();
        let searcher = SearchEngine::new(handler);
        
        let results = searcher.search_literal("hello", false).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].line_number, 0);
    }
    
    #[test]
    fn test_regex_search() {
        let temp_file = create_test_file("test123\nfoo456\ntest789");
        let handler = FileHandler::open(temp_file.path().to_str().unwrap()).unwrap();
        let searcher = SearchEngine::new(handler);
        
        let regex = Regex::new(r"test\d+").unwrap();
        let results = searcher.search_regex(&regex).unwrap();
        assert_eq!(results.len(), 2);
    }
    
    #[test]
    fn test_find_next() {
        let temp_file = create_test_file("apple\nbanana\napple\norange");
        let handler = FileHandler::open(temp_file.path().to_str().unwrap()).unwrap();
        let searcher = SearchEngine::new(handler);
        
        let result = searcher.find_next("apple", 0, true);
        assert!(result.is_some());
        assert_eq!(result.unwrap().line_number, 2);
    }
    
    #[test]
    fn test_find_previous() {
        let temp_file = create_test_file("apple\nbanana\napple\norange");
        let handler = FileHandler::open(temp_file.path().to_str().unwrap()).unwrap();
        let searcher = SearchEngine::new(handler);
        
        let result = searcher.find_previous("apple", 3, true);
        assert!(result.is_some());
        assert_eq!(result.unwrap().line_number, 2);
    }
    
    #[test]
    fn test_count_matches() {
        let temp_file = create_test_file("cat\ndog\ncat\ncat\nbird");
        let handler = FileHandler::open(temp_file.path().to_str().unwrap()).unwrap();
        let searcher = SearchEngine::new(handler);
        
        let count = searcher.count_matches("cat", true).unwrap();
        assert_eq!(count, 3);
    }
}
