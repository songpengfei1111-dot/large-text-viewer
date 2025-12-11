pub mod editor;
pub mod file_handler;
pub mod search;

// Re-export commonly used types
pub use editor::Editor;
pub use file_handler::FileHandler;
pub use search::{SearchEngine, SearchResult};
