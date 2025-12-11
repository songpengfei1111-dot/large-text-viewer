use large_text_viewer::{Editor, FileHandler, SearchEngine, SearchResult};
use iced::widget::{
    button, column, container, row, scrollable, text, text_input, Column, Space,
};
use iced::{executor, Alignment, Application, Command, Element, Length, Settings, Theme};
use std::path::PathBuf;

/// Main GUI application
pub struct TextViewer {
    // File handling
    file_handler: Option<FileHandler>,
    file_path: Option<PathBuf>,
    
    // Viewport
    current_line: usize,
    viewport_size: usize,
    lines_cache: Vec<String>,
    
    // Search
    search_query: String,
    search_results: Vec<SearchResult>,
    current_search_index: Option<usize>,
    case_sensitive: bool,
    
    // Replace
    replace_text: String,
    show_replace: bool,
    
    // UI State
    status_message: String,
    file_input: String,
}

#[derive(Debug, Clone)]
pub enum Message {
    // File operations
    FileInputChanged(String),
    OpenFile,
    FileOpened(Result<FileHandler, String>),
    
    // Navigation
    ScrollUp,
    ScrollDown,
    PageUp,
    PageDown,
    GoToLine(String),
    JumpToLine,
    
    // Search operations
    SearchQueryChanged(String),
    PerformSearch,
    NextMatch,
    PreviousMatch,
    ToggleCaseSensitive,
    
    // Replace operations
    ReplaceTextChanged(String),
    ToggleReplace,
    ReplaceAll,
    ReplaceCurrent,
    
    // General
    ClearStatus,
}

impl Application for TextViewer {
    type Executor = executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        (
            Self {
                file_handler: None,
                file_path: None,
                current_line: 0,
                viewport_size: 50,
                lines_cache: Vec::new(),
                search_query: String::new(),
                search_results: Vec::new(),
                current_search_index: None,
                case_sensitive: false,
                replace_text: String::new(),
                show_replace: false,
                status_message: String::from("No file loaded"),
                file_input: String::new(),
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        String::from("Large Text File Viewer")
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::FileInputChanged(input) => {
                self.file_input = input;
                Command::none()
            }
            
            Message::OpenFile => {
                let path = self.file_input.clone();
                Command::perform(
                    async move {
                        FileHandler::open(&path)
                            .map_err(|e| e.to_string())
                    },
                    Message::FileOpened,
                )
            }
            
            Message::FileOpened(result) => {
                match result {
                    Ok(handler) => {
                        let total_lines = handler.total_lines();
                        let file_size = handler.file_size();
                        
                        self.lines_cache = handler.get_viewport_lines(0, self.viewport_size);
                        self.file_handler = Some(handler);
                        self.file_path = Some(PathBuf::from(self.file_input.clone()));
                        self.current_line = 0;
                        
                        self.status_message = format!(
                            "Loaded: {} lines, {} bytes",
                            total_lines,
                            file_size
                        );
                    }
                    Err(e) => {
                        self.status_message = format!("Error: {}", e);
                    }
                }
                Command::none()
            }
            
            Message::ScrollUp => {
                if self.current_line > 0 {
                    self.current_line = self.current_line.saturating_sub(1);
                    self.update_viewport();
                }
                Command::none()
            }
            
            Message::ScrollDown => {
                if let Some(ref handler) = self.file_handler {
                    if self.current_line + self.viewport_size < handler.total_lines() {
                        self.current_line += 1;
                        self.update_viewport();
                    }
                }
                Command::none()
            }
            
            Message::PageUp => {
                self.current_line = self.current_line.saturating_sub(self.viewport_size);
                self.update_viewport();
                Command::none()
            }
            
            Message::PageDown => {
                if let Some(ref handler) = self.file_handler {
                    let max_line = handler.total_lines().saturating_sub(self.viewport_size);
                    self.current_line = (self.current_line + self.viewport_size).min(max_line);
                    self.update_viewport();
                }
                Command::none()
            }
            
            Message::GoToLine(input) => {
                if let Ok(line_num) = input.parse::<usize>() {
                    if let Some(ref handler) = self.file_handler {
                        if line_num > 0 && line_num <= handler.total_lines() {
                            self.current_line = line_num - 1;
                            self.update_viewport();
                            self.status_message = format!("Jumped to line {}", line_num);
                        }
                    }
                }
                Command::none()
            }
            
            Message::JumpToLine => Command::none(),
            
            Message::SearchQueryChanged(query) => {
                self.search_query = query;
                Command::none()
            }
            
            Message::PerformSearch => {
                if let Some(ref handler) = self.file_handler {
                    if !self.search_query.is_empty() {
                        let searcher = SearchEngine::new(handler.clone());
                        match searcher.search(&self.search_query, self.case_sensitive) {
                            Ok(results) => {
                                let count = results.len();
                                self.search_results = results;
                                self.current_search_index = if count > 0 { Some(0) } else { None };
                                
                                if let Some(idx) = self.current_search_index {
                                    let result = &self.search_results[idx];
                                    self.current_line = result.line_number;
                                    self.update_viewport();
                                }
                                
                                self.status_message = format!("Found {} matches", count);
                            }
                            Err(e) => {
                                self.status_message = format!("Search error: {}", e);
                            }
                        }
                    }
                }
                Command::none()
            }
            
            Message::NextMatch => {
                if let Some(current_idx) = self.current_search_index {
                    if !self.search_results.is_empty() {
                        let next_idx = (current_idx + 1) % self.search_results.len();
                        self.current_search_index = Some(next_idx);
                        
                        let result = &self.search_results[next_idx];
                        self.current_line = result.line_number;
                        self.update_viewport();
                        
                        self.status_message = format!(
                            "Match {} of {}",
                            next_idx + 1,
                            self.search_results.len()
                        );
                    }
                }
                Command::none()
            }
            
            Message::PreviousMatch => {
                if let Some(current_idx) = self.current_search_index {
                    if !self.search_results.is_empty() {
                        let prev_idx = if current_idx == 0 {
                            self.search_results.len() - 1
                        } else {
                            current_idx - 1
                        };
                        self.current_search_index = Some(prev_idx);
                        
                        let result = &self.search_results[prev_idx];
                        self.current_line = result.line_number;
                        self.update_viewport();
                        
                        self.status_message = format!(
                            "Match {} of {}",
                            prev_idx + 1,
                            self.search_results.len()
                        );
                    }
                }
                Command::none()
            }
            
            Message::ToggleCaseSensitive => {
                self.case_sensitive = !self.case_sensitive;
                self.status_message = format!(
                    "Case sensitive: {}",
                    if self.case_sensitive { "ON" } else { "OFF" }
                );
                Command::none()
            }
            
            Message::ReplaceTextChanged(text) => {
                self.replace_text = text;
                Command::none()
            }
            
            Message::ToggleReplace => {
                self.show_replace = !self.show_replace;
                Command::none()
            }
            
            Message::ReplaceAll => {
                if let (Some(ref handler), Some(ref path)) = (&self.file_handler, &self.file_path) {
                    if !self.search_query.is_empty() {
                        let editor = Editor::new(handler.clone());
                        let path_str = path.to_str().unwrap().to_string();
                        let search_query = self.search_query.clone();
                        let replace_text = self.replace_text.clone();
                        let case_sensitive = self.case_sensitive;
                        
                        match editor.replace_all(
                            &path_str,
                            &search_query,
                            &replace_text,
                            case_sensitive,
                        ) {
                            Ok(count) => {
                                self.status_message = format!("Replaced {} occurrences", count);
                                // Reload file
                                return Command::perform(
                                    async move {
                                        FileHandler::open(&path_str)
                                            .map_err(|e| e.to_string())
                                    },
                                    Message::FileOpened,
                                );
                            }
                            Err(e) => {
                                self.status_message = format!("Replace error: {}", e);
                            }
                        }
                    }
                }
                Command::none()
            }
            
            Message::ReplaceCurrent => {
                // TODO: Implement single replacement
                self.status_message = String::from("Single replace not yet implemented");
                Command::none()
            }
            
            Message::ClearStatus => {
                self.status_message.clear();
                Command::none()
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let title = text("Large Text File Viewer")
            .size(24);
        
        // File input section
        let file_input_row = row![
            text("File path:").width(Length::Fixed(80.0)),
            text_input("Enter file path...", &self.file_input)
                .on_input(Message::FileInputChanged)
                .width(Length::Fill),
            button("Open").on_press(Message::OpenFile),
        ]
        .spacing(10)
        .align_items(Alignment::Center);
        
        // Search bar
        let search_row = row![
            text("Search:").width(Length::Fixed(80.0)),
            text_input("Enter search query...", &self.search_query)
                .on_input(Message::SearchQueryChanged)
                .width(Length::Fill),
            button("Find").on_press(Message::PerformSearch),
            button("Next").on_press(Message::NextMatch),
            button("Prev").on_press(Message::PreviousMatch),
            button(if self.case_sensitive { "Aa" } else { "aa" })
                .on_press(Message::ToggleCaseSensitive),
            button(if self.show_replace { "▼" } else { "▶" })
                .on_press(Message::ToggleReplace),
        ]
        .spacing(10)
        .align_items(Alignment::Center);
        
        // Replace bar (conditionally shown)
        let replace_row = if self.show_replace {
            Some(
                row![
                    text("Replace:").width(Length::Fixed(80.0)),
                    text_input("Replacement text...", &self.replace_text)
                        .on_input(Message::ReplaceTextChanged)
                        .width(Length::Fill),
                    button("Replace All").on_press(Message::ReplaceAll),
                ]
                .spacing(10)
                .align_items(Alignment::Center),
            )
        } else {
            None
        };
        
        // Viewport (scrollable text area)
        let mut viewport_content = Column::new().spacing(2);
        
        for (idx, line) in self.lines_cache.iter().enumerate() {
            let line_num = self.current_line + idx + 1;
            let line_text = format!("{:6} | {}", line_num, line);
            viewport_content = viewport_content.push(
                text(line_text)
                    .size(14)
                    .font(iced::Font::MONOSPACE)
            );
        }
        
        let viewport = scrollable(
            container(viewport_content)
                .padding(10)
                .width(Length::Fill)
        )
        .height(Length::Fill);
        
        // Navigation controls
        let nav_row = row![
            button("↑ Line").on_press(Message::ScrollUp),
            button("↓ Line").on_press(Message::ScrollDown),
            button("⇞ Page Up").on_press(Message::PageUp),
            button("⇟ Page Down").on_press(Message::PageDown),
            Space::with_width(Length::Fixed(20.0)),
            text(format!(
                "Line {}-{} of {}",
                self.current_line + 1,
                (self.current_line + self.viewport_size).min(
                    self.file_handler
                        .as_ref()
                        .map(|h| h.total_lines())
                        .unwrap_or(0)
                ),
                self.file_handler
                    .as_ref()
                    .map(|h| h.total_lines())
                    .unwrap_or(0)
            ))
            .size(14),
        ]
        .spacing(10)
        .align_items(Alignment::Center);
        
        // Status bar
        let status_bar = container(
            text(&self.status_message).size(14)
        )
        .padding(5)
        .width(Length::Fill);
        
        // Main layout
        let mut main_column = column![
            title,
            file_input_row,
            search_row,
        ]
        .spacing(10)
        .padding(10);
        
        if let Some(replace) = replace_row {
            main_column = main_column.push(replace);
        }
        
        main_column = main_column
            .push(viewport)
            .push(nav_row)
            .push(status_bar);
        
        container(main_column)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

impl TextViewer {
    fn update_viewport(&mut self) {
        if let Some(ref handler) = self.file_handler {
            self.lines_cache = handler.get_viewport_lines(self.current_line, self.viewport_size);
        }
    }
}

pub fn run() -> iced::Result {
    TextViewer::run(Settings::default())
}
