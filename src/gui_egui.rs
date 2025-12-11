use large_text_viewer::{Editor, FileHandler, SearchEngine, SearchResult};
use eframe::egui;
use std::path::PathBuf;

fn main() -> Result<(), eframe::Error> {
    // Force software rendering for better WSL2 compatibility
    unsafe {
        std::env::set_var("LIBGL_ALWAYS_SOFTWARE", "1");
    }
    
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_title("Large Text File Viewer"),
        renderer: eframe::Renderer::Glow, // Use OpenGL instead of Vulkan
        ..Default::default()
    };

    eframe::run_native(
        "Large Text Viewer",
        options,
        Box::new(|_cc| Ok(Box::new(TextViewerApp::default()))),
    )
}struct TextViewerApp {
    // File handling
    file_handler: Option<FileHandler>,
    file_path: Option<PathBuf>,
    
    // UI state
    file_input: String,
    current_line: usize,
    viewport_size: usize,
    status_message: String,
    
    // Search
    search_query: String,
    search_results: Vec<SearchResult>,
    current_search_index: Option<usize>,
    case_sensitive: bool,
    
    // Replace
    replace_text: String,
    show_replace: bool,
    
    // Cached viewport
    lines_cache: Vec<String>,
}

impl Default for TextViewerApp {
    fn default() -> Self {
        Self {
            file_handler: None,
            file_path: None,
            file_input: String::new(),
            current_line: 0,
            viewport_size: 50,
            status_message: "No file loaded".to_string(),
            search_query: String::new(),
            search_results: Vec::new(),
            current_search_index: None,
            case_sensitive: false,
            replace_text: String::new(),
            show_replace: false,
            lines_cache: Vec::new(),
        }
    }
}

impl TextViewerApp {
    fn update_viewport(&mut self) {
        if let Some(ref handler) = self.file_handler {
            self.lines_cache = handler.get_viewport_lines(self.current_line, self.viewport_size);
        }
    }
    
    fn open_file(&mut self, path: String) {
        match FileHandler::open(&path) {
            Ok(handler) => {
                let total_lines = handler.total_lines();
                let file_size = handler.file_size();
                
                self.lines_cache = handler.get_viewport_lines(0, self.viewport_size);
                self.file_handler = Some(handler);
                self.file_path = Some(PathBuf::from(path));
                self.current_line = 0;
                
                self.status_message = format!(
                    "Loaded: {} lines, {:.2} MB",
                    total_lines,
                    file_size as f64 / 1_048_576.0
                );
            }
            Err(e) => {
                self.status_message = format!("Error: {}", e);
            }
        }
    }
    
    fn perform_search(&mut self) {
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
    }
    
    fn next_match(&mut self) {
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
    }
    
    fn previous_match(&mut self) {
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
    }
    
    fn replace_all(&mut self) {
        if let (Some(ref handler), Some(ref path)) = (&self.file_handler, &self.file_path) {
            if !self.search_query.is_empty() {
                let editor = Editor::new(handler.clone());
                let path_str = path.to_str().unwrap();
                
                match editor.replace_all(
                    path_str,
                    &self.search_query,
                    &self.replace_text,
                    self.case_sensitive,
                ) {
                    Ok(count) => {
                        self.status_message = format!("Replaced {} occurrences", count);
                        // Reload file
                        self.open_file(path_str.to_string());
                    }
                    Err(e) => {
                        self.status_message = format!("Replace error: {}", e);
                    }
                }
            }
        }
    }
}

impl eframe::App for TextViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Top panel with file input
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.heading("Large Text File Viewer");
            ui.add_space(5.0);
            
            ui.horizontal(|ui| {
                ui.label("File path:");
                ui.text_edit_singleline(&mut self.file_input);
                if ui.button("Open").clicked() {
                    let path = self.file_input.clone();
                    self.open_file(path);
                }
            });
        });
        
        // Bottom panel with status
        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status_message);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(ref handler) = self.file_handler {
                        ui.label(format!(
                            "Lines {}-{} of {}",
                            self.current_line + 1,
                            (self.current_line + self.viewport_size).min(handler.total_lines()),
                            handler.total_lines()
                        ));
                    }
                });
            });
        });
        
        // Search/Replace panel
        egui::TopBottomPanel::top("search_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Search:");
                ui.text_edit_singleline(&mut self.search_query);
                if ui.button("Find").clicked() {
                    self.perform_search();
                }
                if ui.button("Next").clicked() {
                    self.next_match();
                }
                if ui.button("Prev").clicked() {
                    self.previous_match();
                }
                ui.checkbox(&mut self.case_sensitive, "Case sensitive");
                if ui.button(if self.show_replace { "▼ Replace" } else { "▶ Replace" }).clicked() {
                    self.show_replace = !self.show_replace;
                }
            });
            
            if self.show_replace {
                ui.horizontal(|ui| {
                    ui.label("Replace:");
                    ui.text_edit_singleline(&mut self.replace_text);
                    if ui.button("Replace All").clicked() {
                        self.replace_all();
                    }
                });
            }
        });
        
        // Navigation panel
        egui::TopBottomPanel::bottom("nav_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("↑ Line").clicked() {
                    self.current_line = self.current_line.saturating_sub(1);
                    self.update_viewport();
                }
                if ui.button("↓ Line").clicked() {
                    if let Some(ref handler) = self.file_handler {
                        if self.current_line + self.viewport_size < handler.total_lines() {
                            self.current_line += 1;
                            self.update_viewport();
                        }
                    }
                }
                if ui.button("⇞ Page Up").clicked() {
                    self.current_line = self.current_line.saturating_sub(self.viewport_size);
                    self.update_viewport();
                }
                if ui.button("⇟ Page Down").clicked() {
                    if let Some(ref handler) = self.file_handler {
                        let max_line = handler.total_lines().saturating_sub(self.viewport_size);
                        self.current_line = (self.current_line + self.viewport_size).min(max_line);
                        self.update_viewport();
                    }
                }
            });
        });
        
        // Central panel with text view
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                    for (idx, line) in self.lines_cache.iter().enumerate() {
                        let line_num = self.current_line + idx + 1;
                        let line_text = format!("{:6} | {}", line_num, line);
                        
                        ui.label(
                            egui::RichText::new(line_text)
                                .font(egui::FontId::monospace(14.0))
                        );
                    }
                });
            });
        });
    }
}
