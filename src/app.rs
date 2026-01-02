mod state;
mod file_ops;
mod search_ops;
mod render;

use eframe::egui;
use encoding_rs::Encoding;
use notify::Watcher;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::{atomic::AtomicBool, Arc};

use large_text_core::file_reader::FileReader;
use large_text_core::line_indexer::LineIndexer;
use large_text_core::replacer::ReplaceMessage;
use large_text_core::search_engine::{SearchEngine, SearchMessage, SearchResult};

pub use state::{ScrollState, SearchState, ReplaceState, UIState, PendingReplacement};

pub struct TextViewerApp {
    pub file_reader: Option<Arc<FileReader>>,
    pub line_indexer: LineIndexer,
    pub search_engine: SearchEngine,

    pub scroll: ScrollState,
    pub search: SearchState,
    pub replace: ReplaceState,
    pub ui: UIState,

    pub goto_line_input: String,
    pub tail_mode: bool,
    pub watcher: Option<Box<dyn Watcher>>,
    pub file_change_rx: Option<Receiver<()>>,
    pub status_message: String,
    pub selected_encoding: &'static Encoding,
    pub unsaved_changes: bool,
    pub pending_replacements: Vec<PendingReplacement>,
    pub open_start_time: Option<std::time::Instant>,
}

impl Default for TextViewerApp {
    fn default() -> Self {
        Self {
            file_reader: None,
            line_indexer: LineIndexer::new(),
            search_engine: SearchEngine::new(),
            scroll: ScrollState::default(),
            search: SearchState::default(),
            replace: ReplaceState::default(),
            ui: UIState::default(),
            goto_line_input: String::new(),
            tail_mode: false,
            watcher: None,
            file_change_rx: None,
            status_message: String::new(),
            selected_encoding: encoding_rs::UTF_8,
            unsaved_changes: false,
            pending_replacements: Vec::new(),
            open_start_time: None,
        }
    }
}

impl TextViewerApp {
    pub fn go_to_line(&mut self) {
        let Ok(line_num) = self.goto_line_input.parse::<usize>() else {
            self.status_message = "Invalid line number".to_string();
            return;
        };

        let total_lines = self.line_indexer.total_lines();
        if line_num == 0 || line_num > total_lines {
            self.status_message = "Line number out of range".to_string();
            return;
        }

        let target_line = line_num.saturating_sub(5);
        self.scroll.line = target_line.saturating_sub(3);
        self.scroll.to_row = Some(target_line);
        self.scroll.pending_target = Some(target_line);
        self.status_message = format!("Jumped to line {}", line_num);
    }
}

impl eframe::App for TextViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_first_frame_timing();
        self.update_window_title(ctx);
        self.handle_keyboard_shortcuts(ctx);
        self.apply_theme(ctx);
        self.poll_background_tasks(ctx);
        self.render_ui(ctx);
    }
}
