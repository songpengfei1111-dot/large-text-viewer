use large_text_viewer::*;
use std::io::Write;
use tempfile::NamedTempFile;

// Helper function to create test files
fn create_test_file(content: &str) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(content.as_bytes()).unwrap();
    file.flush().unwrap();
    file
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    
    #[test]
    fn test_end_to_end_file_operations() {
        let content = "line 1\nline 2\nline 3\nline 4\nline 5\n";
        let temp_file = create_test_file(content);
        let path = temp_file.path().to_str().unwrap();
        
        // Open file
        let handler = file_handler::FileHandler::open(path).unwrap();
        assert_eq!(handler.total_lines(), 5);
        
        // Get viewport
        let viewport = handler.get_viewport_lines(0, 3);
        assert_eq!(viewport.len(), 3);
        assert_eq!(viewport[0], "line 1");
        
        // Modify line
        handler.set_line(0, "modified line".to_string()).unwrap();
        assert_eq!(handler.get_line(0).unwrap(), "modified line");
    }
    
    #[test]
    fn test_search_and_replace_workflow() {
        let content = "apple\nbanana\napple\norange\napple\n";
        let mut temp_file = create_test_file(content);
        let path = temp_file.path().to_str().unwrap();
        
        // Open and search
        let handler = file_handler::FileHandler::open(path).unwrap();
        let searcher = search::SearchEngine::new(handler.clone());
        
        let results = searcher.search("apple", true).unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].line_number, 0);
        assert_eq!(results[1].line_number, 2);
        assert_eq!(results[2].line_number, 4);
        
        // Replace all
        let editor = editor::Editor::new(handler.clone());
        let count = editor.replace_all(path, "apple", "fruit", true).unwrap();
        assert_eq!(count, 3);
        
        // Verify replacement
        let new_handler = file_handler::FileHandler::open(path).unwrap();
        assert_eq!(new_handler.get_line(0).unwrap(), "fruit");
        assert_eq!(new_handler.get_line(2).unwrap(), "fruit");
        assert_eq!(new_handler.get_line(4).unwrap(), "fruit");
        assert_eq!(new_handler.get_line(1).unwrap(), "banana");
    }
    
    #[test]
    fn test_large_viewport_navigation() {
        // Create a larger file
        let mut content = String::new();
        for i in 1..=100 {
            content.push_str(&format!("Line {}\n", i));
        }
        
        let temp_file = create_test_file(&content);
        let path = temp_file.path().to_str().unwrap();
        
        let handler = file_handler::FileHandler::open(path).unwrap();
        assert_eq!(handler.total_lines(), 100);
        
        // Test different viewports
        let viewport1 = handler.get_viewport_lines(0, 10);
        assert_eq!(viewport1.len(), 10);
        assert_eq!(viewport1[0], "Line 1");
        
        let viewport2 = handler.get_viewport_lines(50, 10);
        assert_eq!(viewport2.len(), 10);
        assert_eq!(viewport2[0], "Line 51");
        
        let viewport3 = handler.get_viewport_lines(95, 10);
        assert_eq!(viewport3.len(), 5); // Only 5 lines remain
        assert_eq!(viewport3[0], "Line 96");
    }
    
    #[test]
    fn test_regex_search_and_replace() {
        let content = "test123\nfoo456\ntest789\nbar000\n";
        let mut temp_file = create_test_file(content);
        let path = temp_file.path().to_str().unwrap();
        
        let handler = file_handler::FileHandler::open(path).unwrap();
        let searcher = search::SearchEngine::new(handler.clone());
        
        // Regex search
        let regex = regex::Regex::new(r"test\d+").unwrap();
        let results = searcher.search_regex(&regex).unwrap();
        assert_eq!(results.len(), 2);
        
        // Regex replace
        let editor = editor::Editor::new(handler);
        let count = editor.replace_all(path, r"test(\d+)", "num$1", true).unwrap();
        assert_eq!(count, 2);
        
        // Verify
        let new_handler = file_handler::FileHandler::open(path).unwrap();
        assert_eq!(new_handler.get_line(0).unwrap(), "num123");
        assert_eq!(new_handler.get_line(2).unwrap(), "num789");
    }
    
    #[test]
    fn test_case_insensitive_operations() {
        let content = "Hello World\nhello rust\nHELLO test\n";
        let mut temp_file = create_test_file(content);
        let path = temp_file.path().to_str().unwrap();
        
        let handler = file_handler::FileHandler::open(path).unwrap();
        let searcher = search::SearchEngine::new(handler.clone());
        
        // Case insensitive search
        let results = searcher.search_literal("hello", false).unwrap();
        assert_eq!(results.len(), 3);
        
        // Case insensitive replace
        let editor = editor::Editor::new(handler);
        let count = editor.replace_all(path, "hello", "hi", false).unwrap();
        assert_eq!(count, 3);
    }
    
    #[test]
    fn test_edge_cases() {
        // Empty file
        let empty_file = create_test_file("");
        let handler = file_handler::FileHandler::open(empty_file.path().to_str().unwrap()).unwrap();
        assert_eq!(handler.total_lines(), 1);
        
        // Single line without newline
        let single_line = create_test_file("single line");
        let handler = file_handler::FileHandler::open(single_line.path().to_str().unwrap()).unwrap();
        assert_eq!(handler.total_lines(), 1);
        assert_eq!(handler.get_line(0).unwrap(), "single line");
        
        // File with only newlines
        let newlines = create_test_file("\n\n\n");
        let handler = file_handler::FileHandler::open(newlines.path().to_str().unwrap()).unwrap();
        assert_eq!(handler.total_lines(), 3);
    }
}
