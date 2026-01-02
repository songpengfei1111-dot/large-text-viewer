use eframe::egui;
use encoding_rs::Encoding;
use notify::{RecursiveMode, Result as NotifyResult, Watcher};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use large_text_core::file_reader::{available_encodings, detect_encoding, FileReader};
use large_text_core::line_indexer::LineIndexer;
use large_text_core::replacer::{ReplaceMessage, Replacer};
use large_text_core::search_engine::{SearchEngine, SearchMessage, SearchResult, SearchType};

struct MiniMapRenderer {
    enabled: bool,
    width: f32,
}

impl Default for MiniMapRenderer {
    fn default() -> Self {
        Self {
            enabled: true,
            width: 200.0,
        }
    }
}

impl MiniMapRenderer {
    fn new() -> Self {
        Self::default()
    }

    fn width(&self) -> f32 {
        self.width
    }

    /// 渲染minimap - 只显示当前视口周围7倍范围的内容
    fn render(
        &self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        current_line: usize,
        visible_lines: usize,
        total_lines: usize,
        font_size: f32,
        reader: &Arc<FileReader>,
        indexer: &LineIndexer,
    ) -> Option<usize> {
        if !self.enabled || total_lines == 0 {
            return None;
        }

        let mut target_line = None;

        // 绘制背景
        let painter = ui.painter();
        painter.rect_filled(rect, 2.0, egui::Color32::from_gray(25));
        painter.rect_stroke(rect, 2.0, egui::Stroke::new(1.0, egui::Color32::from_gray(60)));

        // 计算minimap显示范围：当前视口的7倍
        let minimap_range = visible_lines * 7;
        let half_range = minimap_range / 2;
        
        // 计算起始和结束行，确保在文件范围内
        let start_line = current_line.saturating_sub(half_range);
        let end_line = (current_line + visible_lines + half_range).min(total_lines);
        let actual_range = end_line - start_line;

        if actual_range == 0 {
            return None;
        }

        // 计算每行在minimap中的高度
        let available_height = rect.height() - 20.0; // 留出边距
        let line_height = available_height / actual_range as f32;

        // 渲染文本行
        let mini_font_size = (font_size * 0.4).max(6.0);
        let text_color = egui::Color32::from_gray(150);
        let current_viewport_color = egui::Color32::from_gray(200);

        for line_num in start_line..end_line {
            let relative_idx = line_num - start_line;
            let y_pos = rect.top() + 10.0 + relative_idx as f32 * line_height;
            
            if y_pos + line_height > rect.bottom() {
                break;
            }

            // 获取行内容
            if let Some((line_start, line_end)) = indexer.get_line_range(line_num) {
                let line_text = reader.get_chunk(line_start, line_end);
                let trimmed = line_text
                    .trim_end_matches('\n')
                    .trim_end_matches('\r')
                    .chars()
                    .take(30) // 限制显示字符数
                    .collect::<String>();

                // 判断是否在当前视口内
                let is_in_viewport = line_num >= current_line && line_num < current_line + visible_lines;
                let color = if is_in_viewport { current_viewport_color } else { text_color };

                // 绘制行号
                let line_num_text = format!("{:4}", line_num + 1);
                painter.text(
                    egui::pos2(rect.left() + 5.0, y_pos),
                    egui::Align2::LEFT_TOP,
                    line_num_text,
                    egui::FontId::monospace(mini_font_size * 0.8),
                    egui::Color32::from_gray(100),
                );

                // 绘制文本内容
                if !trimmed.is_empty() {
                    painter.text(
                        egui::pos2(rect.left() + 35.0, y_pos),
                        egui::Align2::LEFT_TOP,
                        trimmed,
                        egui::FontId::monospace(mini_font_size),
                        color,
                    );
                }
            }
        }

        // 绘制当前可见区域的高亮框
        let viewport_start_relative = current_line.saturating_sub(start_line);
        let viewport_end_relative = (current_line + visible_lines).saturating_sub(start_line);
        
        let viewport_top = rect.top() + 10.0 + viewport_start_relative as f32 * line_height;
        let viewport_bottom = rect.top() + 10.0 + viewport_end_relative as f32 * line_height;
        let viewport_height = (viewport_bottom - viewport_top).max(8.0);

        let viewport_rect = egui::Rect::from_min_size(
            egui::pos2(rect.left() + 2.0, viewport_top),
            egui::vec2(rect.width() - 4.0, viewport_height),
        );

        // 绘制当前视口高亮
        painter.rect_filled(
            viewport_rect,
            2.0,
            egui::Color32::from_rgba_unmultiplied(255, 255, 255, 40),
        );
        painter.rect_stroke(
            viewport_rect,
            2.0,
            egui::Stroke::new(2.0, egui::Color32::WHITE),
        );

        // 处理点击事件
        let response = ui.interact(rect, ui.id().with("minimap"), egui::Sense::click());
        if response.clicked() {
            if let Some(click_pos) = response.interact_pointer_pos() {
                let relative_y = click_pos.y - rect.top() - 10.0;
                let click_ratio = (relative_y / available_height).clamp(0.0, 1.0);
                
                // 计算点击对应的行号
                let clicked_relative_line = (click_ratio * actual_range as f32) as usize;
                let clicked_line = start_line + clicked_relative_line;
                
                // 如果点击在当前视口上方，向上跳转一个屏幕
                if click_pos.y < viewport_top {
                    target_line = Some(current_line.saturating_sub(visible_lines));
                }
                // 如果点击在当前视口下方，向下跳转一个屏幕
                else if click_pos.y > viewport_bottom {
                    target_line = Some((current_line + visible_lines).min(total_lines.saturating_sub(1)));
                }
                // 如果点击在视口内或其他位置，跳转到点击的具体位置
                else {
                    target_line = Some(clicked_line.min(total_lines.saturating_sub(1)));
                }
            }
        }

        target_line
    }
}

#[derive(Clone)]
struct PendingReplacement {
    offset: usize,
    old_len: usize,
    new_text: String,
}

#[derive(Default)]
struct SearchState {
    query: String,
    use_regex: bool,
    case_sensitive: bool,
    results: Vec<SearchResult>,
    current_index: usize,
    total_results: usize,
    page_start_index: usize,
    page_offsets: Vec<usize>,
    error: Option<String>,
    in_progress: bool,
    find_all: bool,
    message_rx: Option<Receiver<SearchMessage>>,
    cancellation_token: Option<Arc<AtomicBool>>,
    count_done: bool,
    count_start_time: Option<Instant>,
}

impl SearchState {
    fn clear(&mut self) {
        self.results.clear();
        self.current_index = 0;
        self.total_results = 0;
        self.page_start_index = 0;
        self.page_offsets.clear();
        self.error = None;
    }

    fn cancel(&mut self) {
        if let Some(token) = &self.cancellation_token {
            token.store(true, Ordering::Relaxed);
        }
        self.in_progress = false;
    }
}

#[derive(Default)]
struct ReplaceState {
    query: String,
    in_progress: bool,
    message_rx: Option<Receiver<ReplaceMessage>>,
    cancellation_token: Option<Arc<AtomicBool>>,
    progress: Option<f32>,
    status_message: Option<String>,
}

impl ReplaceState {
    fn cancel(&mut self) {
        if let Some(token) = &self.cancellation_token {
            token.store(true, Ordering::Relaxed);
        }
    }

    fn reset(&mut self) {
        self.in_progress = false;
        self.message_rx = None;
        self.cancellation_token = None;
        self.progress = None;
    }
}

struct ScrollState {
    line: usize,
    visible_lines: usize,
    to_row: Option<usize>,
    intra_row_offset_px: f32,
    drag_target_row: Option<usize>,
    drag_grab_offset_px: Option<f32>,
    drag_last_commit: Option<Instant>,
    // 添加平滑滚动支持
    target_line: Option<usize>,
    animation_start_time: Option<Instant>,
    animation_start_line: usize,
    animation_duration: Duration,
}

impl Default for ScrollState {
    fn default() -> Self {
        Self {
            line: 0,
            visible_lines: 50,
            to_row: None,
            intra_row_offset_px: 0.0,
            drag_target_row: None,
            drag_grab_offset_px: None,
            drag_last_commit: None,
        }
    }
}

impl ScrollState {
    fn reset(&mut self) {
        self.line = 0;
        self.to_row = Some(0);
        self.intra_row_offset_px = 0.0;
        self.drag_target_row = None;
        self.drag_grab_offset_px = None;
        self.drag_last_commit = None;
    }
}

struct ViewSettings {
    font_size: f32,
    wrap_mode: bool,
    dark_mode: bool,
    show_line_numbers: bool,
    show_minimap: bool,
}

impl Default for ViewSettings {
    fn default() -> Self {
        Self {
            font_size: 14.0,
            wrap_mode: false,
            dark_mode: true,
            show_line_numbers: true,
            show_minimap: true,
        }
    }
}

struct TailMode {
    enabled: bool,
    watcher: Option<Box<dyn Watcher>>,
    change_rx: Option<Receiver<()>>,
}

impl Default for TailMode {
    fn default() -> Self {
        Self {
            enabled: false,
            watcher: None,
            change_rx: None,
        }
    }
}

impl TailMode {
    fn disable(&mut self) {
        self.enabled = false;
        self.watcher = None;
        self.change_rx = None;
    }
}

pub struct TextViewerApp {
    file_reader: Option<Arc<FileReader>>,
    line_indexer: LineIndexer,
    search_engine: SearchEngine,
    minimap: MiniMapRenderer,
    // 窗口实例的数据
    search: SearchState,
    replace: ReplaceState,
    scroll: ScrollState,
    view: ViewSettings,
    tail: TailMode,

    show_search_bar: bool,
    show_replace: bool,
    show_file_info: bool,
    show_encoding_selector: bool,
    focus_search_input: bool,

    goto_line_input: String,
    status_message: String,
    selected_encoding: &'static Encoding,

    unsaved_changes: bool,
    pending_replacements: Vec<PendingReplacement>,

    open_start_time: Option<Instant>,
}

impl Default for TextViewerApp {
    fn default() -> Self {
        Self {
            file_reader: None,
            line_indexer: LineIndexer::new(),
            search_engine: SearchEngine::new(),
            minimap: MiniMapRenderer::new(),
            search: SearchState::default(),
            replace: ReplaceState::default(),
            scroll: ScrollState::default(),
            view: ViewSettings::default(),
            tail: TailMode::default(),
            show_search_bar: false,
            show_replace: false,
            show_file_info: false,
            show_encoding_selector: false,
            focus_search_input: false,
            goto_line_input: String::new(),
            status_message: String::new(),
            selected_encoding: encoding_rs::UTF_8,
            unsaved_changes: false,
            pending_replacements: Vec::new(),
            open_start_time: None,
        }
    }
}

impl TextViewerApp {
    fn open_file(&mut self, path: PathBuf) {
        self.open_start_time = Some(Instant::now());

        let reader = match FileReader::new(path.clone(), self.selected_encoding) {
            Ok(r) => r,
            Err(e) => {
                self.status_message = format!("Error opening file: {}", e);
                return;
            }
        };

        self.file_reader = Some(Arc::new(reader));
        self.line_indexer
            .index_file(self.file_reader.as_ref().unwrap());

        self.scroll.reset();
        self.status_message = format!("Opened: {}", path.display());

        self.search_engine.clear();
        self.search.clear();

        if self.tail.enabled {
            self.setup_file_watcher();
        }
    }

    fn setup_file_watcher(&mut self) {
        let reader = match &self.file_reader {
            Some(r) => r,
            None => return,
        };

        let (tx, rx) = channel();
        let path = reader.path().clone();

        let watcher = match notify::recommended_watcher(move |res: NotifyResult<notify::Event>| {
            if res.is_ok() {
                let _ = tx.send(());
            }
        }) {
            Ok(w) => w,
            Err(_) => return,
        };

        let mut watcher = watcher;
        if watcher.watch(&path, RecursiveMode::NonRecursive).is_ok() {
            self.tail.watcher = Some(Box::new(watcher));
            self.tail.change_rx = Some(rx);
        }
    }

    fn check_file_changes(&mut self) {
        let rx = match &self.tail.change_rx {
            Some(r) => r,
            None => return,
        };

        if rx.try_recv().is_err() {
            return;
        }

        let reader = match &self.file_reader {
            Some(r) => r,
            None => return,
        };

        let path = reader.path().clone();
        let encoding = reader.encoding();
        self.selected_encoding = encoding;
        self.open_file(path);

        if self.tail.enabled {
            let total_lines = self.line_indexer.total_lines();
            let target = total_lines.saturating_sub(self.scroll.visible_lines);
            self.scroll.line = target;
            self.scroll.to_row = Some(target);
        }
    }

    fn perform_search(&mut self, find_all: bool) {
        self.search.error = None;
        self.search.clear();
        self.search_engine.clear();

        if self.search.in_progress {
            self.status_message = "Search already running...".to_string();
            return;
        }

        let reader = match &self.file_reader {
            Some(r) => r.clone(),
            None => {
                self.status_message = "Open a file before searching".to_string();
                return;
            }
        };

        if self.search.query.is_empty() {
            self.status_message = "Enter a search query first".to_string();
            return;
        }

        self.search_engine.set_query(
            self.search.query.clone(),
            self.search.use_regex,
            self.search.case_sensitive,
        );

        let (tx, rx) = std::sync::mpsc::sync_channel(10_000);
        self.search.message_rx = Some(rx);
        self.search.in_progress = true;
        self.search.find_all = find_all;
        self.search.count_done = false;

        let cancel_token = Arc::new(AtomicBool::new(false));
        self.search.cancellation_token = Some(cancel_token.clone());

        self.status_message = if find_all {
            "Searching all matches...".to_string()
        } else {
            "Searching first match...".to_string()
        };

        let query = self.search.query.clone();
        let use_regex = self.search.use_regex;
        let case_sensitive = self.search.case_sensitive;

        if find_all {
            self.search.count_start_time = Some(Instant::now());

            let tx_count = tx.clone();
            let reader_count = reader.clone();
            let query_count = query.clone();
            let cancel_count = cancel_token.clone();

            std::thread::spawn(move || {
                let mut engine = SearchEngine::new();
                engine.set_query(query_count, use_regex, case_sensitive);
                engine.count_matches(reader_count, tx_count, cancel_count);
            });

            let tx_fetch = tx;
            let reader_fetch = reader;
            let query_fetch = query;
            let cancel_fetch = cancel_token;

            std::thread::spawn(move || {
                let mut engine = SearchEngine::new();
                engine.set_query(query_fetch, use_regex, case_sensitive);
                engine.fetch_matches(reader_fetch, tx_fetch, 0, 1000, cancel_fetch);
            });
        } else {
            let tx_fetch = tx;
            let reader_fetch = reader;
            let cancel_fetch = cancel_token;

            std::thread::spawn(move || {
                let mut engine = SearchEngine::new();
                engine.set_query(query, use_regex, case_sensitive);
                engine.fetch_matches(reader_fetch, tx_fetch, 0, 1, cancel_fetch);
            });
        }
    }

    fn poll_search_results(&mut self) {
        if !self.search.in_progress {
            return;
        }

        if self.search.message_rx.is_none() {
            return;
        }

        let mut new_results = false;
        let mut should_cancel = false;
        let mut disconnected = false;

        if let Some(ref rx) = self.search.message_rx {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    SearchMessage::CountResult(count) => {
                        self.search.total_results += count;
                        if self.search.find_all {
                            self.status_message =
                                format!("Found {} matches...", self.search.total_results);
                        }
                    }
                    SearchMessage::ChunkResult(chunk) => {
                        self.search.results.extend(chunk.matches);
                        new_results = true;
                    }
                    SearchMessage::Done(search_type) => {
                        if let SearchType::Count = search_type {
                            self.search.count_done = true;
                            if let Some(start) = self.search.count_start_time.take() {
                                let elapsed = start.elapsed();
                                self.status_message =
                                    format!("{} (Counted in {:.2?})", self.status_message, elapsed);
                            }
                        }

                        if self.search.find_all
                            && self.search.count_done
                            && self.search.results.len() == self.search.total_results
                        {
                            should_cancel = true;
                        }
                    }
                    SearchMessage::Error(e) => {
                        self.search.in_progress = false;
                        self.search.message_rx = None;
                        self.search.error = Some(e.clone());
                        self.status_message = format!("Search failed: {}", e);
                        return;
                    }
                }
            }

            if let Err(std::sync::mpsc::TryRecvError::Disconnected) = rx.try_recv() {
                disconnected = true;
            }
        }

        if should_cancel {
            self.search.cancel();
        }

        if disconnected {
            self.search.in_progress = false;
            self.search.message_rx = None;
            self.search.results.sort_by_key(|r| r.byte_offset);

            if !self.search.find_all {
                self.search.total_results = self.search.results.len();
            } else {
                self.search.total_results =
                    self.search.total_results.max(self.search.results.len());
            }

            self.update_search_status_message();
            self.scroll_to_first_result();
        }

        if new_results {
            self.search.results.sort_by_key(|r| r.byte_offset);
            if self.scroll.to_row.is_none()
                && !self.search.results.is_empty()
                && self.search.current_index == 0
            {
                self.scroll_to_first_result();
            }
        }
    }

    fn update_search_status_message(&mut self) {
        let total = self.search.total_results;
        if total > 0 {
            self.status_message = if self.search.find_all {
                format!("Found {} matches", total)
            } else {
                "Showing first match. Run Find All to see every result.".to_string()
            };
        } else {
            self.status_message = "No matches found".to_string();
        }
    }

    fn scroll_to_first_result(&mut self) {
        if self.scroll.to_row.is_some() || self.search.results.is_empty() {
            return;
        }
        let target = self
            .line_indexer
            .find_line_at_offset(self.search.results[0].byte_offset);
        self.scroll.line = target;
        self.scroll.to_row = Some(target);
    }

    fn poll_replace_results(&mut self) {
        if !self.replace.in_progress {
            return;
        }

        let rx = match &self.replace.message_rx {
            Some(r) => r,
            None => return,
        };

        let mut done = false;

        while let Ok(msg) = rx.try_recv() {
            match msg {
                ReplaceMessage::Progress(processed, total) => {
                    let progress = processed as f32 / total as f32;
                    self.replace.progress = Some(progress);
                    self.replace.status_message =
                        Some(format!("Replacing... {:.1}%", progress * 100.0));
                }
                ReplaceMessage::Done => {
                    self.replace.status_message = Some("Replacement complete.".to_string());
                    self.status_message = "Replacement complete.".to_string();
                    done = true;
                }
                ReplaceMessage::Error(e) => {
                    self.replace.status_message = Some(format!("Replace failed: {}", e));
                    self.status_message = format!("Replace failed: {}", e);
                    done = true;
                }
            }
        }

        if done {
            self.replace.reset();
        }
    }

    fn perform_single_replace(&mut self) {
        if self.search.results.is_empty() {
            return;
        }

        let local_index = match self
            .search
            .current_index
            .checked_sub(self.search.page_start_index)
        {
            Some(i) => i,
            None => return,
        };

        if local_index >= self.search.results.len() {
            return;
        }

        let match_info = self.search.results[local_index].clone();

        self.pending_replacements.push(PendingReplacement {
            offset: match_info.byte_offset,
            old_len: match_info.match_len,
            new_text: self.replace.query.clone(),
        });
        self.unsaved_changes = true;
        self.status_message = "Replacement pending. Save to apply changes.".to_string();
    }

    fn save_file(&mut self) {
        let reader = match &self.file_reader {
            Some(r) => r,
            None => return,
        };

        let input_path = reader.path().clone();
        let encoding = reader.encoding();

        let output_path = match rfd::FileDialog::new()
            .set_file_name(input_path.file_name().unwrap().to_string_lossy())
            .save_file()
        {
            Some(p) => p,
            None => return,
        };

        if output_path == input_path {
            self.save_to_same_file(&input_path, encoding);
        } else {
            self.save_to_different_file(&input_path, &output_path);
        }
    }

    fn save_to_same_file(&mut self, path: &PathBuf, encoding: &'static Encoding) {
        self.file_reader = None;

        let success = self.apply_replacements(path);

        if success {
            self.pending_replacements.clear();
            self.unsaved_changes = false;
            self.status_message = "File saved successfully".to_string();
        }

        match FileReader::new(path.clone(), encoding) {
            Ok(reader) => {
                self.file_reader = Some(Arc::new(reader));
                self.line_indexer
                    .index_file(self.file_reader.as_ref().unwrap());
                self.perform_search(self.search.find_all);
            }
            Err(e) => {
                self.status_message = format!("Error re-opening file: {}", e);
            }
        }
    }

    fn save_to_different_file(&mut self, input_path: &PathBuf, output_path: &PathBuf) {
        if std::fs::copy(input_path, output_path).is_err() {
            self.status_message = "Error copying file for save".to_string();
            return;
        }

        let success = self.apply_replacements(output_path);

        if success {
            self.pending_replacements.clear();
            self.unsaved_changes = false;
            self.status_message = "File saved successfully".to_string();
            self.open_file(output_path.clone());
        }
    }

    fn apply_replacements(&mut self, path: &PathBuf) -> bool {
        for replacement in &self.pending_replacements {
            if let Err(e) = Replacer::replace_single(
                path,
                replacement.offset,
                replacement.old_len,
                &replacement.new_text,
            ) {
                self.status_message = format!("Error saving: {}", e);
                return false;
            }
        }
        true
    }

    fn perform_replace(&mut self) {
        if self.replace.in_progress {
            return;
        }

        let reader = match &self.file_reader {
            Some(r) => r,
            None => return,
        };

        let input_path = reader.path().clone();

        let output_path = match rfd::FileDialog::new()
            .set_file_name(format!(
                "{}.modified",
                input_path.file_name().unwrap().to_string_lossy()
            ))
            .save_file()
        {
            Some(p) => p,
            None => return,
        };

        let query = self.search.query.clone();
        let replace_with = self.replace.query.clone();
        let use_regex = self.search.use_regex;

        let (tx, rx) = std::sync::mpsc::channel();
        self.replace.message_rx = Some(rx);
        self.replace.in_progress = true;
        self.replace.progress = Some(0.0);
        self.replace.status_message = None;

        let cancel_token = Arc::new(AtomicBool::new(false));
        self.replace.cancellation_token = Some(cancel_token.clone());

        std::thread::spawn(move || {
            Replacer::replace_all(
                &input_path,
                &output_path,
                &query,
                &replace_with,
                use_regex,
                tx,
                cancel_token,
            );
        });
    }

    fn go_to_next_result(&mut self) {
        if self.search.total_results == 0 {
            return;
        }

        let next = (self.search.current_index + 1) % self.search.total_results;
        self.navigate_to_result(next);
    }

    fn go_to_previous_result(&mut self) {
        if self.search.total_results == 0 {
            return;
        }

        let prev = if self.search.current_index == 0 {
            self.search.total_results - 1
        } else {
            self.search.current_index - 1
        };

        self.navigate_to_result(prev);
    }

    fn navigate_to_result(&mut self, target_index: usize) {
        let page_end = self.search.page_start_index + self.search.results.len();

        if target_index >= self.search.page_start_index && target_index < page_end {
            self.search.current_index = target_index;
            let local = target_index - self.search.page_start_index;
            let result = &self.search.results[local];
            let target_line = self.line_indexer.find_line_at_offset(result.byte_offset);
            self.scroll.line = target_line;
            self.scroll.to_row = Some(target_line);
            return;
        }

        if target_index == 0 {
            self.fetch_page(0, 0);
        } else if target_index < self.search.page_start_index {
            let page_idx = target_index / 1000;
            let page_start = page_idx * 1000;
            let offset = self.search.page_offsets.get(page_idx).copied().unwrap_or(0);
            self.fetch_page(page_start, offset);
        } else if let Some(last) = self.search.results.last() {
            let start_offset = last.byte_offset + 1;
            self.fetch_page(target_index, start_offset);
        }

        self.search.current_index = target_index;
    }

    fn fetch_page(&mut self, start_index: usize, start_offset: usize) {
        if self.search.in_progress {
            return;
        }

        let reader = match &self.file_reader {
            Some(r) => r.clone(),
            None => return,
        };

        self.search.results.clear();
        self.search.page_start_index = start_index;

        let page_idx = start_index / 1000;
        if page_idx >= self.search.page_offsets.len() {
            self.search.page_offsets.push(start_offset);
        } else {
            self.search.page_offsets[page_idx] = start_offset;
        }

        let query = self.search.query.clone();
        let use_regex = self.search.use_regex;
        let case_sensitive = self.search.case_sensitive;

        let (tx, rx) = std::sync::mpsc::sync_channel(10_000);
        self.search.message_rx = Some(rx);
        self.search.in_progress = true;

        let cancel_token = Arc::new(AtomicBool::new(false));
        self.search.cancellation_token = Some(cancel_token.clone());

        self.status_message = format!(
            "Loading results {}...{}",
            start_index + 1,
            start_index + 1000
        );

        std::thread::spawn(move || {
            let mut engine = SearchEngine::new();
            engine.set_query(query, use_regex, case_sensitive);
            engine.fetch_matches(reader, tx, start_offset, 1000, cancel_token);
        });
    }

    fn go_to_line(&mut self) {
        let line_num = match self.goto_line_input.parse::<usize>() {
            Ok(n) => n,
            Err(_) => {
                self.status_message = "Invalid line number".to_string();
                return;
            }
        };

        if line_num == 0 || line_num > self.line_indexer.total_lines() {
            self.status_message = "Line number out of range".to_string();
            return;
        }

        let target = line_num.saturating_sub(1);
        self.scroll.line = target.saturating_sub(3);
        self.scroll.to_row = Some(target);
        self.status_message = format!("Jumped to line {}", line_num);
    }

    fn render_menu_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                self.render_file_menu(ui, ctx);
                self.render_view_menu(ui);
                self.render_search_menu(ui);
                self.render_tools_menu(ui);
            });
        });
    }

    fn render_file_menu(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.menu_button("File", |ui| {
            if ui.button("Open...").clicked() {
                self.handle_open_file();
                ui.close_menu();
            }

            if ui
                .add_enabled(self.unsaved_changes, egui::Button::new("Save (Ctrl+S)"))
                .clicked()
            {
                self.save_file();
                ui.close_menu();
            }

            if ui.button("File Info").clicked() {
                self.show_file_info = !self.show_file_info;
                ui.close_menu();
            }

            if ui.button("Exit").clicked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
    }

    fn handle_open_file(&mut self) {
        let path = match rfd::FileDialog::new().pick_file() {
            Some(p) => p,
            None => return,
        };

        if let Ok(mut file) = std::fs::File::open(&path) {
            let mut buffer = [0; 4096];
            if let Ok(n) = std::io::Read::read(&mut file, &mut buffer) {
                self.selected_encoding = detect_encoding(&buffer[..n]);
            }
        }
        self.open_file(path);
    }

    fn render_view_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("View", |ui| {
            ui.checkbox(&mut self.view.wrap_mode, "Word Wrap");
            ui.checkbox(&mut self.view.show_line_numbers, "Line Numbers");
            ui.checkbox(&mut self.view.show_minimap, "Show Minimap");
            ui.checkbox(&mut self.view.dark_mode, "Dark Mode");
            ui.separator();
            ui.label("Font Size:");
            ui.add(egui::Slider::new(&mut self.view.font_size, 8.0..=32.0));
            ui.separator();
            if ui.button("Select Encoding").clicked() {
                self.show_encoding_selector = true;
                ui.close_menu();
            }
        });
    }

    fn render_search_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Search", |ui| {
            if ui
                .add(egui::Button::new("Find").shortcut_text("Ctrl+F"))
                .clicked()
            {
                self.show_search_bar = true;
                self.focus_search_input = true;
                ui.close_menu();
            }
            if ui
                .add(egui::Button::new("Replace").shortcut_text("Ctrl+R"))
                .clicked()
            {
                self.show_search_bar = true;
                self.show_replace = !self.show_replace;
                ui.close_menu();
            }
            ui.separator();
            ui.checkbox(&mut self.search.use_regex, "Use Regex");
            ui.checkbox(&mut self.search.case_sensitive, "Match Case");
        });
    }

    fn render_tools_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Tools", |ui| {
            if ui
                .checkbox(&mut self.tail.enabled, "Tail Mode (Auto-refresh)")
                .changed()
            {
                if self.tail.enabled {
                    self.setup_file_watcher();
                } else {
                    self.tail.disable();
                }
            }
        });
    }

    fn render_toolbar(&mut self, ctx: &egui::Context) {
        if !self.show_search_bar {
            return;
        }

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            self.render_search_toolbar(ui);
            self.render_replace_toolbar(ui);
            self.render_search_error(ui);
        });
    }

    fn render_search_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Search:");
            let response =
                ui.add(egui::TextEdit::singleline(&mut self.search.query).desired_width(300.0));

            if self.focus_search_input {
                response.request_focus();
                self.focus_search_input = false;
            }

            ui.checkbox(&mut self.search.case_sensitive, "Aa")
                .on_hover_text("Match Case");
            ui.checkbox(&mut self.search.use_regex, ".*")
                .on_hover_text("Use Regex");

            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.perform_search(false);
            }

            if ui
                .add_enabled(!self.search.in_progress, egui::Button::new("🔍 Find"))
                .clicked()
            {
                self.perform_search(false);
            }

            if ui
                .add_enabled(!self.search.in_progress, egui::Button::new("🔎 Find All"))
                .clicked()
            {
                self.perform_search(true);
            }

            if ui.button("⬆ Previous").clicked() {
                self.go_to_previous_result();
            }

            if ui.button("⬇ Next").clicked() {
                self.go_to_next_result();
            }

            self.render_search_progress(ui);
            self.render_search_counter(ui);

            ui.separator();
            self.render_goto_line(ui);
        });
    }

    fn render_search_progress(&mut self, ui: &mut egui::Ui) {
        if !self.search.in_progress {
            return;
        }

        ui.add(egui::Spinner::new().size(18.0));
        ui.label("Searching...");

        if ui.button("Stop").clicked() {
            self.search.cancel();
            self.status_message = "Search stopped by user".to_string();
        }
    }

    fn render_search_counter(&self, ui: &mut egui::Ui) {
        if self.search.total_results == 0 {
            return;
        }
        let current = (self.search.current_index + 1).min(self.search.total_results);
        ui.label(format!("{}/{}", current, self.search.total_results));
    }

    fn render_goto_line(&mut self, ui: &mut egui::Ui) {
        ui.label("Go to line:");
        let response =
            ui.add(egui::TextEdit::singleline(&mut self.goto_line_input).desired_width(80.0));

        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            self.go_to_line();
        }

        if ui.button("Go").clicked() {
            self.go_to_line();
        }
    }

    fn render_replace_toolbar(&mut self, ui: &mut egui::Ui) {
        if !self.show_replace {
            return;
        }

        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Replace with:");
            ui.add(
                egui::TextEdit::singleline(&mut self.replace.query)
                    .desired_width(200.0)
                    .hint_text("Replacement text..."),
            );

            if self.replace.in_progress {
                if ui.button("Stop Replace").clicked() {
                    self.replace.cancel();
                }
                ui.spinner();
                if let Some(progress) = self.replace.progress {
                    ui.label(format!("{:.1}%", progress * 100.0));
                }
            } else {
                if ui.button("Replace").clicked() {
                    self.perform_single_replace();
                }
                if ui.button("Replace All").clicked() {
                    self.perform_replace();
                }
            }
        });

        if let Some(ref msg) = self.replace.status_message {
            ui.label(msg);
        }
    }

    fn render_search_error(&self, ui: &mut egui::Ui) {
        if let Some(ref error) = self.search.error {
            ui.colored_label(egui::Color32::RED, format!("Search error: {}", error));
        }
    }

    fn render_status_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some(ref reader) = self.file_reader {
                    ui.label(format!("File: {}", reader.path().display()));
                    ui.separator();
                    ui.label(format!("Size: {} bytes", reader.len()));
                    ui.separator();
                    ui.label(format!("Lines: ~{}", self.line_indexer.total_lines()));
                    ui.separator();
                    ui.label(format!("Encoding: {}", reader.encoding().name()));
                    ui.separator();
                    ui.label(format!("Line: {}", self.scroll.line + 1));
                } else {
                    ui.label("No file opened - Click File → Open to start");
                }

                if !self.status_message.is_empty() {
                    ui.separator();
                    ui.label(&self.status_message);
                }
            });
        });
    }

    fn render_text_area(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.file_reader.is_none() {
                ui.centered_and_justified(|ui| {
                    ui.heading("Large Text Viewer");
                    ui.label("\nClick File → Open to load a text file");
                });
                return;
            }

            // 如果启用了minimap，使用侧边面板布局
            if self.view.show_minimap {
                egui::SidePanel::right("minimap_panel")
                    .resizable(true)
                    .default_width(self.minimap.width())
                    .width_range(150.0..=400.0)
                    .show_inside(ui, |ui| {
                        if let Some(ref reader) = self.file_reader {
                            let minimap_rect = ui.available_rect_before_wrap();
                            
                            // 渲染minimap并处理点击
                            if let Some(target_line) = self.minimap.render(
                                ui,
                                minimap_rect,
                                self.scroll.line,
                                self.scroll.visible_lines,
                                self.line_indexer.total_lines(),
                                self.view.font_size,
                                reader,
                                &self.line_indexer,
                            ) {
                                self.scroll.line = target_line;
                                self.scroll.to_row = Some(target_line);
                            }
                        }
                    });

                // 主文本区域占用剩余空间
                egui::CentralPanel::default().show_inside(ui, |ui| {
                    self.render_file_content(ui);
                });
            } else {
                // 没有minimap时的正常渲染
                self.render_file_content(ui);
            }
        });
    }

    fn render_file_content(&mut self, ui: &mut egui::Ui) {
        let font_id = egui::FontId::monospace(self.view.font_size);
        let line_height = ui.fonts(|f| f.row_height(&font_id));
        let total_lines = self.line_indexer.total_lines();

        if total_lines == 0 { return; }

        let spacing = ui.spacing().item_spacing.y;
        let row_height = line_height + spacing;
        let available_height = ui.available_height();

        self.scroll.visible_lines = ((available_height / row_height).ceil() as usize)
            .saturating_add(2)
            .max(1);

        if let Some(target) = self.scroll.to_row.take() {
            self.scroll.line = target.min(total_lines - 1);
            self.scroll.intra_row_offset_px = 0.0;
        }

        if self.scroll.line >= total_lines {
            self.scroll.line = total_lines - 1;
            self.scroll.intra_row_offset_px = 0.0;
        }

        let scrollbar_width: f32 = 14.0;
        let available_rect = ui.available_rect_before_wrap();
        ui.allocate_rect(available_rect, egui::Sense::hover());

        let (content_rect, scrollbar_rect) =
            self.calculate_layout_rects(available_rect, scrollbar_width);

        self.render_scrollbar(ui, scrollbar_rect, total_lines, row_height);
        self.handle_scroll_input(ui, content_rect, row_height, total_lines);
        self.render_content(ui, content_rect, total_lines, row_height);
    }

    fn calculate_layout_rects(
        &self,
        available_rect: egui::Rect,
        scrollbar_width: f32,
    ) -> (egui::Rect, egui::Rect) {
        let scrollbar_width = scrollbar_width.min(available_rect.width().max(0.0));
        let scrollbar_rect = egui::Rect::from_min_max(
            egui::pos2(available_rect.right() - scrollbar_width, available_rect.top()),
            egui::pos2(available_rect.right(), available_rect.bottom()),
        );
        let content_rect = egui::Rect::from_min_max(
            available_rect.left_top(),
            egui::pos2(scrollbar_rect.left(), available_rect.bottom()),
        );
        (content_rect, scrollbar_rect)
    }

    fn render_scrollbar(
        &mut self,
        ui: &mut egui::Ui,
        scrollbar_rect: egui::Rect,
        total_lines: usize,
        _row_height: f32,
    ) {
        let reader = self.file_reader.as_ref().unwrap();
        let scrollbar_id = ui.make_persistent_id(("global_scrollbar", reader.path()));
        let response = ui.interact(scrollbar_rect, scrollbar_id, egui::Sense::click_and_drag());

        let total_f64 = (total_lines - 1) as f64;
        let mut fraction = if total_lines > 1 {
            (self.scroll.line as f64 / total_f64).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let track_h = scrollbar_rect.height();
        let handle_h = if total_lines > 0 {
            (track_h * (self.scroll.visible_lines as f32 / total_lines as f32)).clamp(18.0, track_h)
        } else {
            track_h
        };

        let handle_travel = (track_h - handle_h).max(1.0);
        let handle_top = scrollbar_rect.top() + (fraction as f32) * handle_travel;

        let handle_rect = egui::Rect::from_min_size(
            egui::pos2(scrollbar_rect.left(), handle_top),
            egui::vec2(scrollbar_rect.width(), handle_h),
        );

        let painter = ui.painter();
        painter.rect_filled(scrollbar_rect, 4.0, egui::Color32::from_gray(30));
        painter.rect_filled(handle_rect, 4.0, egui::Color32::from_gray(90));

        if let Some(pointer_pos) = response.interact_pointer_pos() {
            let grab_offset = self.scroll.drag_grab_offset_px.get_or_insert_with(|| {
                if handle_rect.contains(pointer_pos) {
                    pointer_pos.y - handle_rect.top()
                } else {
                    handle_h / 2.0
                }
            });

            let new_top = (pointer_pos.y - *grab_offset)
                .clamp(scrollbar_rect.top(), scrollbar_rect.top() + handle_travel);
            let new_fraction = ((new_top - scrollbar_rect.top()) / handle_travel) as f64;
            fraction = new_fraction.clamp(0.0, 1.0);

            let target = if total_lines > 1 {
                (fraction * total_f64).round() as usize
            } else {
                0
            };
            self.scroll.drag_target_row = Some(target.min(total_lines - 1));
        } else {
            self.scroll.drag_grab_offset_px = None;
        }

        if response.drag_stopped() {
            if let Some(target) = self.scroll.drag_target_row.take() {
                self.scroll.line = target;
                self.scroll.intra_row_offset_px = 0.0;
            }
            self.scroll.drag_last_commit = None;
        }

        if let Some(target) = self.scroll.drag_target_row {
            let should_commit = self
                .scroll
                .drag_last_commit
                .map_or(true, |t| t.elapsed() >= Duration::from_millis(50));

            if should_commit {
                self.scroll.line = target;
                self.scroll.intra_row_offset_px = 0.0;
                self.scroll.drag_last_commit = Some(Instant::now());
            }
        }
    }

    fn handle_scroll_input(
        &mut self,
        ui: &mut egui::Ui,
        content_rect: egui::Rect,
        row_height: f32,
        total_lines: usize,
    ) {
        let over_content = ui
            .input(|i| i.pointer.hover_pos())
            .map_or(false, |p| content_rect.contains(p));

        if !over_content || self.scroll.drag_target_row.is_some() {
            return;
        }

        let delta = ui.input(|i| i.smooth_scroll_delta.y);
        if delta.abs() < 0.01 {
            return;
        }

        self.scroll.intra_row_offset_px -= delta;

        if row_height <= 0.0 {
            return;
        }

        let delta_rows = (self.scroll.intra_row_offset_px / row_height).floor() as i64;
        if delta_rows != 0 {
            self.scroll.intra_row_offset_px -= (delta_rows as f32) * row_height;
            let new_row = (self.scroll.line as i64 + delta_rows).clamp(0, (total_lines - 1) as i64);
            self.scroll.line = new_row as usize;
        }

        self.scroll.intra_row_offset_px = self
            .scroll
            .intra_row_offset_px
            .clamp(0.0, row_height.max(0.0));
    }

    fn render_content(
        &mut self,
        ui: &mut egui::Ui,
        content_rect: egui::Rect,
        total_lines: usize,
        row_height: f32,
    ) {
        let reader = self.file_reader.as_ref().unwrap().clone();
        let path_str = reader.path().display().to_string();

        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(content_rect), |ui| {
            egui::ScrollArea::horizontal()
                .id_salt(path_str)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_clip_rect(content_rect);

                    let start = self.scroll.line.min(total_lines - 1);
                    let end = (start + self.scroll.visible_lines).min(total_lines);
                    let render_height =
                        ((end - start) as f32) * row_height + self.scroll.intra_row_offset_px;

                    let shifted_rect = egui::Rect::from_min_size(
                        egui::pos2(
                            ui.max_rect().left(),
                            content_rect.top() - self.scroll.intra_row_offset_px,
                        ),
                        egui::vec2(
                            ui.max_rect().width(),
                            render_height.max(content_rect.height()),
                        ),
                    );

                    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(shifted_rect), |ui| {
                        for line_num in start..end {
                            self.render_line(ui, &reader, line_num);
                        }
                    });
                });
        });
    }

    fn render_line(&self, ui: &mut egui::Ui, reader: &Arc<FileReader>, line_num: usize) {
        let (start, mut end) = match self.line_indexer.get_line_range(line_num) {
            Some(r) => r,
            None => return,
        };

        if end == usize::MAX {
            end = reader.len();
        }

        if start >= reader.len() {
            return;
        }

        let mut line_text = reader.get_chunk(start, end);

        for replacement in &self.pending_replacements {
            let rep_end = replacement.offset + replacement.old_len;
            if replacement.offset >= start && rep_end <= end {
                let rel_start = replacement.offset - start;
                let rel_end = rep_end - start;
                if line_text.is_char_boundary(rel_start) && line_text.is_char_boundary(rel_end) {
                    line_text.replace_range(rel_start..rel_end, &replacement.new_text);
                }
            }
        }

        let text = line_text
            .trim_end_matches('\n')
            .trim_end_matches('\r');

        let matches = self.collect_line_matches(text, start, end);

        ui.push_id(line_num, |ui| {
            ui.horizontal(|ui| {
                if self.view.show_line_numbers {
                    let ln = egui::RichText::new(format!("{:6} ", line_num + 1))
                        .monospace()
                        .color(egui::Color32::DARK_GRAY);
                    ui.add(egui::Label::new(ln).selectable(false));
                }

                let label = self.build_line_label(ui, text, &matches);
                if label.hovered() {
                    ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::Text);
                }
                label.surrender_focus();
            });
        });
    }

    fn collect_line_matches(
        &self,
        text: &str,
        start: usize,
        end: usize,
    ) -> Vec<(usize, usize, bool)> {
        let mut matches = Vec::new();

        let selected_offset = if self.search.total_results > 0
            && self.search.current_index >= self.search.page_start_index
        {
            let idx = self.search.current_index - self.search.page_start_index;
            self.search.results.get(idx).map(|r| r.byte_offset)
        } else {
            None
        };

        if self.search.find_all {
            for (m_start, m_end) in self.search_engine.find_in_text(text) {
                let abs = start + m_start;
                let selected = Some(abs) == selected_offset;
                matches.push((m_start, m_end, selected));
            }
        } else {
            let start_idx = self
                .search
                .results
                .partition_point(|r| r.byte_offset < start);

            for (idx, res) in self.search.results.iter().enumerate().skip(start_idx) {
                if res.byte_offset >= end {
                    break;
                }

                let rel_start = res.byte_offset.saturating_sub(start);
                if rel_start >= text.len() {
                    continue;
                }

                let rel_end = (rel_start + res.match_len).min(text.len());
                let global_idx = self.search.page_start_index + idx;
                let selected = global_idx == self.search.current_index;
                matches.push((rel_start, rel_end, selected));
            }
        }

        matches
    }

    fn build_line_label(
        &self,
        ui: &mut egui::Ui,
        text: &str,
        matches: &[(usize, usize, bool)],
    ) -> egui::Response {
        if matches.is_empty() {
            let rich = egui::RichText::new(text)
                .monospace()
                .size(self.view.font_size);

            return if self.view.wrap_mode {
                ui.add(egui::Label::new(rich).wrap())
            } else {
                ui.add(egui::Label::new(rich).extend())
            };
        }

        let mut job = egui::text::LayoutJob::default();
        let mut last_end = 0;

        let normal_color = if self.view.dark_mode {
            egui::Color32::LIGHT_GRAY
        } else {
            egui::Color32::BLACK
        };

        for (start, end, selected) in matches {
            if *start > last_end {
                job.append(
                    &text[last_end..*start],
                    0.0,
                    egui::TextFormat {
                        font_id: egui::FontId::monospace(self.view.font_size),
                        color: normal_color,
                        ..Default::default()
                    },
                );
            }

            let match_end = (*end).min(text.len());
            job.append(
                &text[*start..match_end],
                0.0,
                egui::TextFormat {
                    font_id: egui::FontId::monospace(self.view.font_size),
                    color: egui::Color32::BLACK,
                    background: if *selected {
                        egui::Color32::from_rgb(255, 200, 0)
                    } else {
                        egui::Color32::YELLOW
                    },
                    ..Default::default()
                },
            );

            last_end = match_end;
        }

        if last_end < text.len() {
            job.append(
                &text[last_end..],
                0.0,
                egui::TextFormat {
                    font_id: egui::FontId::monospace(self.view.font_size),
                    color: normal_color,
                    ..Default::default()
                },
            );
        }

        if self.view.wrap_mode {
            job.wrap = egui::text::TextWrapping {
                max_width: ui.available_width(),
                ..Default::default()
            };
        }

        ui.add(egui::Label::new(job).extend())
    }

    fn render_encoding_selector(&mut self, ctx: &egui::Context) {
        if !self.show_encoding_selector {
            return;
        }

        egui::Window::new("Select Encoding")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                for (name, encoding) in available_encodings() {
                    if ui
                        .selectable_label(std::ptr::eq(self.selected_encoding, encoding), name)
                        .clicked()
                    {
                        self.selected_encoding = encoding;

                        if let Some(ref reader) = self.file_reader {
                            let path = reader.path().clone();
                            self.open_file(path);
                        }

                        self.show_encoding_selector = false;
                    }
                }

                if ui.button("Cancel").clicked() {
                    self.show_encoding_selector = false;
                }
            });
    }

    fn render_file_info(&mut self, ctx: &egui::Context) {
        if !self.show_file_info {
            return;
        }

        let reader = match &self.file_reader {
            Some(r) => r,
            None => return,
        };

        egui::Window::new("File Information")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(format!("Path: {}", reader.path().display()));
                ui.label(format!(
                    "Size: {} bytes ({:.2} MB)",
                    reader.len(),
                    reader.len() as f64 / 1_000_000.0
                ));
                ui.label(format!("Lines: ~{}", self.line_indexer.total_lines()));
                ui.label(format!("Encoding: {}", reader.encoding().name()));

                if ui.button("Close").clicked() {
                    self.show_file_info = false;
                }
            });
    }

    fn handle_keyboard_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::S)) {
            self.save_file();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::R)) {
            self.show_search_bar = true;
            self.show_replace = !self.show_replace;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::F)) {
            self.show_search_bar = !self.show_search_bar;
            if self.show_search_bar {
                self.focus_search_input = true;
            }
        }
    }

    fn apply_theme(&self, ctx: &egui::Context) {
        if self.view.dark_mode {
            ctx.set_visuals(egui::Visuals::dark());
        } else {
            ctx.set_visuals(egui::Visuals::light());
        }
    }

    fn update_window_title(&self, ctx: &egui::Context) {
        let title = if self.unsaved_changes {
            "Large Text Viewer *"
        } else {
            "Large Text Viewer"
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.to_string()));
    }
}

impl eframe::App for TextViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 记录启动时间
        if let Some(start) = self.open_start_time.take() {
            let elapsed = start.elapsed();
            self.status_message = format!("{} (Rendered in {:.2?})", self.status_message, elapsed);
        }

        self.update_window_title(ctx);
        self.handle_keyboard_shortcuts(ctx);
        self.apply_theme(ctx);

        if self.tail.enabled {
            self.check_file_changes();
            ctx.request_repaint();
        }

        self.poll_search_results();
        self.poll_replace_results();

        if self.search.in_progress || self.replace.in_progress {
            ctx.request_repaint();
        }

        self.render_menu_bar(ctx);
        self.render_toolbar(ctx);
        self.render_status_bar(ctx);
        self.render_encoding_selector(ctx);
        self.render_file_info(ctx);
        self.render_text_area(ctx);
    }
}
