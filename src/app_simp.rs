use eframe::egui;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::sync::atomic::{AtomicBool, Ordering};

use large_text_core::file_reader::FileReader;
use large_text_core::line_indexer::LineIndexer;
use large_text_core::text_cache::TextCache;
use large_text_core::search_engine::{SearchEngine, SearchMessage, SearchResult, SearchType};


/// 精简的Minimap，只与TextCache交互
struct MiniMap {
    enabled: bool,
    width: f32,
}

impl Default for MiniMap {
    fn default() -> Self {
        Self {
            enabled: true,
            width: 200.0,
        }
    }
}

impl MiniMap {
    /// 渲染minimap并返回点击的目标行
    fn render(
        &mut self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        current_line: usize,
        visible_lines: usize,
        text_cache: &mut TextCache,
    ) -> Option<usize> {
        let total_lines = text_cache.total_lines();
        if !self.enabled || total_lines == 0 {
            return None;
        }

        // 绘制背景
        let painter = ui.painter();
        painter.rect_filled(rect, 2.0, egui::Color32::from_gray(25));
        painter.rect_stroke(rect, 2.0, egui::Stroke::new(1.0, egui::Color32::from_gray(60)));

        // 计算显示范围
        let minimap_range = visible_lines * 5;
        let half_range = minimap_range / 2;
        let start_line = current_line.saturating_sub(half_range);
        let end_line = (current_line + visible_lines + half_range).min(total_lines);

        // 渲染文本
        self.render_text(ui, rect, start_line, end_line, current_line, visible_lines, text_cache);

        // 处理点击
        self.handle_click(ui, rect, start_line, end_line, total_lines)
    }

    /// 渲染文本内容
    fn render_text(
        &self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        start_line: usize,
        end_line: usize,
        current_line: usize,
        visible_lines: usize,
        text_cache: &mut TextCache,
    ) {

        let mini_font_size = 2.0;  // 更小的字体
        let available_height = rect.height();
        let actual_range = end_line - start_line;
        let _line_height = available_height / actual_range as f32;


        // 批量获取文本行
        let lines = text_cache.get_lines(start_line, end_line);

        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = egui::Vec2::new(0.0, 0.15);  // 减少行间距

                for (idx, line_text) in lines.iter().enumerate() {
                    let line_num = start_line + idx;
                    let is_in_viewport = line_num >= current_line && line_num < current_line + visible_lines;

                    // 更鲜明的颜色对比
                    let text_color = if is_in_viewport {
                        egui::Color32::from_gray(240)  // 更亮的颜色
                    } else {
                        egui::Color32::from_gray(100)  // 更暗的颜色，增强对比度
                    };

                    // 富文本设置
                    let rich_text = egui::RichText::new(
                        if line_text.trim().is_empty() { " " } else { line_text.trim_end_matches('\n') }
                    )
                        .font(egui::FontId::monospace(mini_font_size))
                        .color(text_color)
                        .strong();  // 加粗提高锐度

                    ui.add(egui::Label::new(rich_text)
                        .wrap_mode(egui::TextWrapMode::Extend));

                }
            });
        });
    }

    /// 处理点击事件
    fn handle_click(
        &self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        start_line: usize,
        end_line: usize,
        total_lines: usize,
    ) -> Option<usize> {
        let response = ui.interact(rect, ui.id().with("minimap"), egui::Sense::click());
        if response.clicked() {
            if let Some(click_pos) = response.interact_pointer_pos() {
                let available_height = rect.height() - 20.0;
                let actual_range = end_line - start_line;
                let relative_y = click_pos.y - rect.top() - 10.0;
                let click_ratio = (relative_y / available_height).clamp(0.0, 1.0);
                
                let clicked_relative_line = (click_ratio * actual_range as f32) as usize;
                let clicked_line = start_line + clicked_relative_line;
                
                return Some(clicked_line.min(total_lines.saturating_sub(1)));
            }
        }
        None
    }
}

/// 简化的搜索状态
struct SearchState {
    query: String,
    results: Vec<SearchResult>,
    current_index: usize,
    total_results: usize,
    in_progress: bool,
    message_rx: Option<Receiver<SearchMessage>>,
    cancellation_token: Option<Arc<AtomicBool>>,
    show_bar: bool,
    use_regex: bool,
    case_sensitive: bool,
    count_done: bool,    // 计数任务是否完成
    fetch_done: bool,    // 获取任务是否完成
    is_find_all: bool,   // 是否是 Find All 模式
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            query: String::new(),
            results: Vec::new(),
            current_index: 0,
            total_results: 0,
            in_progress: false,
            message_rx: None,
            cancellation_token: None,
            show_bar: false,
            use_regex: false,
            case_sensitive: false,
            count_done: false,
            fetch_done: false,
            is_find_all: false,
        }
    }
}

impl SearchState {
    fn reset(&mut self) {
        self.results.clear();
        self.current_index = 0;
        self.total_results = 0;
        self.in_progress = false;
        self.message_rx = None;
        self.cancellation_token = None;
        self.count_done = false;
        self.fetch_done = false;
        self.is_find_all = false;
    }
}

/// 简化的滚动状态
struct ScrollState {
    line: usize,
    visible_lines: usize,
    // 滚动条拖拽
    drag_target_row: Option<usize>,
    drag_grab_offset_px: Option<f32>,
}

impl Default for ScrollState {
    fn default() -> Self {
        Self {
            line: 0,
            visible_lines: 50,
            drag_target_row: None,
            drag_grab_offset_px: None,
        }
    }
}

impl ScrollState {
    fn reset(&mut self) {
        self.line = 0;
        self.drag_target_row = None;
        self.drag_grab_offset_px = None;
    }

    /// 跳转到指定行
    fn jump_to(&mut self, target_line: usize) {
        self.line = target_line;
    }
}

/// 精简的视图设置
struct ViewSettings {
    font_size: f32,
    show_line_numbers: bool,
    show_minimap: bool,
}

impl Default for ViewSettings {
    fn default() -> Self {
        Self {
            font_size: 14.0,
            show_line_numbers: true,
            show_minimap: true,
        }
    }
}

/// 精简的文本查看器应用
pub struct TextViewerAppSimp {
    text_cache: TextCache,
    line_indexer: LineIndexer,
    minimap: MiniMap,
    scroll: ScrollState,
    view: ViewSettings,
    search: SearchState,
    search_engine: SearchEngine,
    status_message: String,
}

impl Default for TextViewerAppSimp {
    fn default() -> Self {
        Self {
            text_cache: TextCache::new(2000), // 缓存2000行
            line_indexer: LineIndexer::new(),
            minimap: MiniMap::default(),
            scroll: ScrollState::default(),
            view: ViewSettings::default(),
            search: SearchState::default(),
            search_engine: SearchEngine::new(),
            status_message: String::new(),
        }
    }
}

impl TextViewerAppSimp {
    fn input_new_file(&mut self, ctx: &egui::Context) {
        ctx.input(|i| {
            if let Some(file) = i.raw.dropped_files.get(0) {
                if let Some(path) = &file.path {
                    self.open_file(path.clone());
                }
            }
        });
    }

    /// 打开文件
    fn open_file(&mut self, path: PathBuf) {
        match FileReader::new(path.clone(), encoding_rs::UTF_8) {
            Ok(reader) => {
                self.line_indexer.index_file(&reader);
                // 创建一个新的 LineIndexer 来传递给 TextCache
                let mut cache_indexer = LineIndexer::new();
                cache_indexer.index_file(&reader);
                self.text_cache.set_file(Arc::new(reader), cache_indexer);
                self.scroll.reset();
                self.search.reset();
                self.search_engine.clear();
                self.status_message = format!("Opened: {}", path.display());
            }
            Err(e) => {
                self.status_message = format!("Error opening file: {}", e);
            }
        }
    }

    /// 渲染菜单栏
    fn render_menu_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open...").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_file() {
                            self.open_file(path);
                        }
                        ui.close_menu();
                    }
                    if ui.button("Exit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                ui.menu_button("View", |ui| {
                    ui.checkbox(&mut self.view.show_line_numbers, "Line Numbers");
                    ui.checkbox(&mut self.view.show_minimap, "Show Minimap");
                    ui.separator();
                    ui.label("Font Size:");
                    ui.add(egui::Slider::new(&mut self.view.font_size, 8.0..=32.0));
                });

                ui.menu_button("Search", |ui| {
                    if ui.button("Find (Ctrl+F)").clicked() {
                        self.search.show_bar = true;
                        ui.close_menu();
                    }
                    ui.separator();
                    ui.checkbox(&mut self.search.use_regex, "Use Regex");
                    ui.checkbox(&mut self.search.case_sensitive, "Match Case");
                });
            });
        });
    }

    /// 渲染搜索栏
    fn render_search_bar(&mut self, ctx: &egui::Context) {
        if !self.search.show_bar {
            return;
        }

        egui::TopBottomPanel::bottom("search_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Search:");
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.search.query)
                        .desired_width(300.0)
                        .hint_text("Enter search term...")
                );

                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    self.perform_search_single();
                }

                ui.checkbox(&mut self.search.case_sensitive, "Aa")
                    .on_hover_text("Match Case");
                ui.checkbox(&mut self.search.use_regex, ".*")
                    .on_hover_text("Use Regex");

                if ui.add_enabled(!self.search.in_progress, egui::Button::new("🔍 Find")).clicked() {
                    self.perform_search_single();
                }

                if ui.add_enabled(!self.search.in_progress, egui::Button::new("🔎 Find All")).clicked() {
                    self.perform_search_all();
                }

                if self.search.total_results > 0 {
                    if ui.button("⬆ Previous").clicked() {
                        self.go_to_previous_result();
                    }
                    if ui.button("⬇ Next").clicked() {
                        self.go_to_next_result();
                    }

                    let current = (self.search.current_index + 1).min(self.search.total_results);
                    ui.label(format!("{}/{}", current, self.search.total_results));
                }

                if self.search.in_progress {
                    ui.spinner();
                    ui.label("Searching...");
                    if ui.button("Stop").clicked() {
                        if let Some(token) = &self.search.cancellation_token {
                            token.store(true, Ordering::Relaxed);
                        }
                        self.search.in_progress = false;
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("✖").clicked() {
                        self.search.show_bar = false;
                    }
                });
            });
        });
    }

    /// 执行单个搜索（只找第一个匹配）
    fn perform_search_single(&mut self) {
        self.perform_search_internal(false);
    }

    /// 执行全部搜索（找所有匹配）
    fn perform_search_all(&mut self) {
        self.perform_search_internal(true);
    }

    /// 执行搜索的内部实现
    fn perform_search_internal(&mut self, find_all: bool) {
        if self.search.query.is_empty() {
            self.status_message = "Enter a search query first".to_string();
            return;
        }

        let Some(reader) = self.text_cache.get_file_reader() else {
            self.status_message = "Open a file before searching".to_string();
            return;
        };

        if self.search.in_progress {
            return;
        }

        self.search.reset();
        self.search.is_find_all = find_all;
        self.search_engine.clear();
        self.search_engine.set_query(
            self.search.query.clone(),
            self.search.use_regex,
            self.search.case_sensitive,
        );

        let reader_arc = reader.clone();
        let (tx, rx) = std::sync::mpsc::sync_channel(10_000); // 增加缓冲区大小
        self.search.message_rx = Some(rx);
        self.search.in_progress = true;

        let cancel_token = Arc::new(AtomicBool::new(false));
        self.search.cancellation_token = Some(cancel_token.clone());

        let query = self.search.query.clone();
        let use_regex = self.search.use_regex;
        let case_sensitive = self.search.case_sensitive;

        if find_all {
            // Find All: 启动两个任务
            // 1. 计数任务：统计所有匹配的数量
            let tx_count = tx.clone();
            let reader_count = reader_arc.clone();
            let query_count = query.clone();
            let cancel_token_count = cancel_token.clone();

            std::thread::spawn(move || {
                let mut engine = SearchEngine::new();
                engine.set_query(query_count, use_regex, case_sensitive);
                engine.count_matches(reader_count, tx_count, cancel_token_count);
            });

            // 2. 获取任务：获取第一页结果（1000个）
            let tx_fetch = tx.clone();
            let reader_fetch = reader_arc.clone();
            let query_fetch = query.clone();
            let cancel_token_fetch = cancel_token.clone();

            std::thread::spawn(move || {
                let mut engine = SearchEngine::new();
                engine.set_query(query_fetch, use_regex, case_sensitive);
                // 只获取第一页的1000个结果，而不是所有结果
                engine.fetch_matches(reader_fetch, tx_fetch, 0, 1000, cancel_token_fetch);
            });

            self.status_message = "Searching all matches...".to_string();
        } else {
            // Find: 只查找第一个匹配
            std::thread::spawn(move || {
                let mut engine = SearchEngine::new();
                engine.set_query(query, use_regex, case_sensitive);
                engine.fetch_matches(reader_arc, tx, 0, 1, cancel_token);
            });

            self.status_message = "Searching first match...".to_string();
        }
    }

    /// 轮询搜索结果
    fn poll_search_results(&mut self) {
        if !self.search.in_progress {
            return;
        }

        let mut new_results_added = false;
        let mut channel_disconnected = false;
        
        if let Some(ref rx) = self.search.message_rx {
            // 处理所有可用的消息
            loop {
                match rx.try_recv() {
                    Ok(msg) => {
                        match msg {
                            SearchMessage::CountResult(count) => {
                                self.search.total_results += count;
                                if self.search.is_find_all {
                                    self.status_message = format!("Found {} matches...", self.search.total_results);
                                }
                            }
                            SearchMessage::ChunkResult(chunk_result) => {
                                self.search.results.extend(chunk_result.matches);
                                new_results_added = true;
                                
                                // 动态显示当前结果数量
                                if self.search.is_find_all {
                                    self.status_message = format!("Found {} matches...", self.search.results.len());
                                }
                            }
                            SearchMessage::Done(search_type) => {
                                match search_type {
                                    SearchType::Count => {
                                        self.search.count_done = true;
                                        println!("Count task completed, total: {}", self.search.total_results);
                                    }
                                    SearchType::Fetch => {
                                        self.search.fetch_done = true;
                                        println!("Fetch task completed, results: {}", self.search.results.len());
                                    }
                                }

                                // 对于Find All模式，当计数和获取任务都完成时，搜索就完成了
                                // 不需要等待获取所有结果，因为我们只获取第一页
                                if self.search.is_find_all && self.search.count_done && self.search.fetch_done {
                                    // 取消任何剩余的任务
                                    if let Some(token) = &self.search.cancellation_token {
                                        token.store(true, Ordering::Relaxed);
                                    }
                                }
                            }
                            SearchMessage::Error(e) => {
                                self.search.in_progress = false;
                                self.search.message_rx = None;
                                self.status_message = format!("Search failed: {}", e);
                                println!("Search error: {}", e);
                                return; // 停止处理消息
                            }
                        }
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        // 没有更多消息，退出循环
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        // 通道断开，搜索完成
                        channel_disconnected = true;
                        break;
                    }
                }
            }
        }

        // 如果通道断开，完成搜索
        if channel_disconnected {
            self.finalize_search();
        }

        if new_results_added {
            // 每帧处理完所有可用块后只排序一次
            self.search.results.sort_by_key(|r| r.byte_offset);
            
            // 如果这是第一批结果，跳转到第一个匹配
            if self.search.current_index == 0 && !self.search.results.is_empty() {
                self.jump_to_current_result();
            }
        }
    }

    /// 完成搜索的最终处理
    fn finalize_search(&mut self) {
        self.search.in_progress = false;
        self.search.message_rx = None;
        
        // 最终排序确保结果有序
        self.search.results.sort_by_key(|r| r.byte_offset);
        
        // 设置最终的总结果数
        if !self.search.is_find_all {
            self.search.total_results = self.search.results.len();
        } else {
            // 确保总数至少是我们获取到的结果数
            self.search.total_results = self.search.total_results.max(self.search.results.len());
        }
        
        let total = self.search.total_results;
        if total > 0 {
            if self.search.is_find_all {
                self.status_message = format!("Found {} matches", total);
            } else {
                self.status_message = "Showing first match. Run Find All to see every result.".to_string();
            }
            
            // 跳转到第一个结果
            if !self.search.results.is_empty() {
                self.jump_to_current_result();
            }
        } else {
            self.status_message = "No matches found".to_string();
        }
        
        println!("Search completed: {} results", total);
    }

    /// 跳转到下一个搜索结果
    fn go_to_next_result(&mut self) {
        if self.search.results.is_empty() {
            return;
        }
        self.search.current_index = (self.search.current_index + 1) % self.search.results.len();
        self.jump_to_current_result();
    }

    /// 跳转到上一个搜索结果
    fn go_to_previous_result(&mut self) {
        if self.search.results.is_empty() {
            return;
        }
        self.search.current_index = if self.search.current_index == 0 {
            self.search.results.len() - 1
        } else {
            self.search.current_index - 1
        };
        self.jump_to_current_result();
    }

    /// 跳转到当前搜索结果
    fn jump_to_current_result(&mut self) {
        if let Some(result) = self.search.results.get(self.search.current_index) {
            let target_line = self.line_indexer.find_line_at_offset(result.byte_offset);
            self.scroll.jump_to(target_line);
        }
    }

    /// 渲染搜索结果面板
    fn render_search_results_panel(&mut self, ctx: &egui::Context) {
        // 只有在搜索栏打开且有搜索结果或正在搜索时才显示面板
        if !self.search.show_bar || (self.search.results.is_empty() && !self.search.in_progress) {
            return;
        }

        egui::SidePanel::left("search_results")
            .default_width(400.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Find Results");

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("✖").clicked() {
                            self.search.show_bar = false;
                        }
                    });
                });

                ui.separator();

                // 显示搜索统计信息
                if self.search.in_progress {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        if self.search.total_results > 0 {
                            ui.label(format!(
                                "Found {} occurrences of '{}'...",
                                self.search.total_results, self.search.query
                            ));
                        } else {
                            ui.label("Searching...");
                        }
                    });
                } else if self.search.total_results > 0 {
                    ui.label(
                        egui::RichText::new(format!(
                            "Found {} occurrences of '{}'.",
                            self.search.total_results, self.search.query
                        ))
                        .strong(),
                    );
                } else if !self.search.query.is_empty() {
                    ui.label("No matches found");
                }

                ui.separator();

                // 渲染搜索结果列表
                if !self.search.results.is_empty() {
                    self.render_search_results_list(ui);
                }
            });
    }

    /// 渲染搜索结果列表（使用虚拟滚动）
    fn render_search_results_list(&mut self, ui: &mut egui::Ui) {
        let reader = match self.text_cache.get_file_reader() {
            Some(r) => r.clone(),
            None => return,
        };

        let text_height = ui.text_style_height(&egui::TextStyle::Monospace);
        let total_results = self.search.results.len();
        let current_index = self.search.current_index;

        if total_results == 0 {
            return;
        }

        // 使用 egui 的虚拟滚动功能
        egui::ScrollArea::both()
            .auto_shrink([false; 2])
            .show_rows(
                ui,
                text_height,
                total_results,
                |ui, row_range| {
                    // 只为可见的行准备数据
                    for idx in row_range {
                        if idx >= total_results {
                            break;
                        }

                        let result = &self.search.results[idx];
                        let is_current = idx == current_index;
                        let line_num = self.line_indexer.find_line_at_offset(result.byte_offset);
                        
                        // 懒加载：只为可见行读取文本内容
                        let (line_text, match_start, match_end) = self.get_search_result_text(&reader, result);
                        
                        // 构建带高亮的文本
                        let job = self.build_search_result_job(
                            &line_text,
                            line_num,
                            match_start,
                            match_end,
                            is_current,
                        );

                        let response = ui.selectable_label(is_current, job);

                        // 点击跳转到该结果
                        if response.clicked() {
                            self.search.current_index = idx;
                            self.jump_to_current_result();
                        }
                    }
                },
            );
    }

    /// 获取搜索结果的文本内容（懒加载）
    fn get_search_result_text(&self, reader: &Arc<FileReader>, result: &SearchResult) -> (String, usize, usize) {
        // 读取匹配周围的上下文
        let context_size = 500;
        let read_start = result.byte_offset.saturating_sub(context_size);
        let read_end = (result.byte_offset + result.match_len + context_size).min(reader.len());
        let chunk = reader.get_chunk(read_start, read_end);
        
        // 找到匹配在 chunk 中的位置
        let match_offset_in_chunk = result.byte_offset - read_start;
        
        // 提取一行文本（从换行符到换行符）
        let line_start_in_chunk = chunk[..match_offset_in_chunk]
            .rfind('\n')
            .map(|pos| pos + 1)
            .unwrap_or(0);
        
        let line_end_in_chunk = chunk[match_offset_in_chunk..]
            .find('\n')
            .map(|pos| match_offset_in_chunk + pos)
            .unwrap_or(chunk.len());
        
        let line_text = chunk[line_start_in_chunk..line_end_in_chunk].to_string();
        
        // 匹配在 line_text 中的位置
        let match_start_in_line = match_offset_in_chunk - line_start_in_chunk;
        let match_end_in_line = match_start_in_line + result.match_len;
        
        (line_text, match_start_in_line, match_end_in_line)
    }

    /// 构建搜索结果的文本作业
    fn build_search_result_job(
        &self,
        line_text: &str,
        line_num: usize,
        match_start: usize,
        match_end: usize,
        is_current: bool,
    ) -> egui::text::LayoutJob {
        let mut job = egui::text::LayoutJob::default();
        job.wrap.max_width = f32::INFINITY; // 禁止换行

        // 文本颜色
        let text_color = egui::Color32::LIGHT_GRAY;
        let line_num_color = egui::Color32::GRAY;

        let highlight_bg = if is_current {
            egui::Color32::from_rgb(80, 80, 120) // 当前选中的结果
        } else {
            egui::Color32::from_rgb(60, 60, 40) // 普通高亮
        };

        // 行号
        job.append(
            &format!("Line {:8}    ", line_num + 1),
            0.0,
            egui::TextFormat {
                font_id: egui::FontId::monospace(self.view.font_size),
                color: line_num_color,
                ..Default::default()
            },
        );

        // 安全地分割文本
        let safe_start = match_start.min(line_text.len());
        let safe_end = match_end.min(line_text.len());

        // 匹配前的文本
        if safe_start > 0 {
            job.append(
                &line_text[..safe_start],
                0.0,
                egui::TextFormat {
                    font_id: egui::FontId::monospace(self.view.font_size),
                    color: text_color,
                    ..Default::default()
                },
            );
        }

        // 匹配的文本（高亮）
        if safe_start < safe_end && safe_end <= line_text.len() {
            job.append(
                &line_text[safe_start..safe_end],
                0.0,
                egui::TextFormat {
                    font_id: egui::FontId::monospace(self.view.font_size),
                    color: egui::Color32::WHITE,
                    background: highlight_bg,
                    ..Default::default()
                },
            );
        }

        // 匹配后的文本
        if safe_end < line_text.len() {
            job.append(
                &line_text[safe_end..],
                0.0,
                egui::TextFormat {
                    font_id: egui::FontId::monospace(self.view.font_size),
                    color: text_color,
                    ..Default::default()
                },
            );
        }

        job
    }
    fn handle_keyboard_shortcuts(&mut self, ctx: &egui::Context) {
        // Ctrl+F / Cmd+F: 切换搜索栏
        if ctx.input_mut(|i| {
            i.consume_key(egui::Modifiers::CTRL, egui::Key::F)
                || i.consume_key(egui::Modifiers::MAC_CMD, egui::Key::F)
        }) {
            self.search.show_bar = !self.search.show_bar;
        }

        // Escape: 关闭搜索栏
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            if self.search.show_bar {
                self.search.show_bar = false;
            }
        }
    }

    fn render_status_bar(&self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if let Some(file_path) = self.text_cache.file_path() {
                    ui.label(format!("File: {}", file_path.display()));
                    ui.separator();
                    ui.label(format!("Lines: {}", self.text_cache.total_lines()));
                    ui.separator();
                    ui.label(format!("Current Line: {}", self.scroll.line + 1));
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

    /// 渲染文本区域
    fn render_text_area(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.text_cache.total_lines() == 0 {
                ui.centered_and_justified(|ui| {
                    ui.heading("Simple Text Viewer");
                    ui.label("\nClick File → Open to load a text file");
                });
                return;
            }

            self.render_file_content(ui);
        });
    }

    /// 渲染文件内容
    fn render_file_content(&mut self, ui: &mut egui::Ui) {
        let font_id = egui::FontId::monospace(self.view.font_size);
        let line_height = ui.fonts(|f| f.row_height(&font_id));
        let total_lines = self.text_cache.total_lines();

        if total_lines == 0 { return; }

        let available_height = ui.available_height();
        let row_height = line_height + ui.spacing().item_spacing.y;
        self.scroll.visible_lines = ((available_height / row_height).ceil() as usize).max(1);

        let available_rect = ui.available_rect_before_wrap();
        let scrollbar_width = 8.0;
        let minimap_width = if self.view.show_minimap { self.minimap.width } else { 0.0 };
        
        let (content_rect, scrollbar_rect, minimap_rect) = self.calculate_layout_rects(
            available_rect, 
            scrollbar_width, 
            minimap_width
        );

        // 渲染minimap
        if self.view.show_minimap {
            if let Some(target_line) = self.minimap.render(
                ui,
                minimap_rect,
                self.scroll.line,
                self.scroll.visible_lines,
                &mut self.text_cache,
            ) {
                self.scroll.jump_to(target_line);
            }
        }

        // 渲染滚动条
        self.render_scrollbar(ui, scrollbar_rect, total_lines);

        // 处理滚动输入
        self.handle_scroll_input(ui, content_rect, row_height, total_lines);

        // 渲染文本内容
        self.render_content(ui, content_rect, total_lines);
    }

    /// 计算布局矩形
    fn calculate_layout_rects(
        &self,
        available_rect: egui::Rect,
        scrollbar_width: f32,
        minimap_width: f32,
    ) -> (egui::Rect, egui::Rect, egui::Rect) {
        // 滚动条在最右边
        let scrollbar_rect = egui::Rect::from_min_max(
            egui::pos2(available_rect.right() - scrollbar_width, available_rect.top()),
            available_rect.right_bottom(),
        );
        
        // minimap在滚动条左边
        let minimap_rect = if minimap_width > 0.0 {
            egui::Rect::from_min_max(
                egui::pos2(scrollbar_rect.left() - minimap_width, available_rect.top()),
                egui::pos2(scrollbar_rect.left(), available_rect.bottom()),
            )
        } else {
            egui::Rect::NOTHING
        };
        
        // 内容区域占用剩余空间
        let content_right = if minimap_width > 0.0 { 
            minimap_rect.left() 
        } else { 
            scrollbar_rect.left() 
        };
        let content_rect = egui::Rect::from_min_max(
            available_rect.left_top(),
            egui::pos2(content_right, available_rect.bottom()),
        );
        
        (content_rect, scrollbar_rect, minimap_rect)
    }

    /// 渲染滚动条
    fn render_scrollbar(&mut self, ui: &mut egui::Ui, scrollbar_rect: egui::Rect, total_lines: usize) {
        let scrollbar_id = ui.make_persistent_id("scrollbar");
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

        // 处理拖拽
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
                self.scroll.jump_to(target);
            }
        }

        // 实时更新拖拽位置
        if let Some(target) = self.scroll.drag_target_row {
            self.scroll.jump_to(target);
        }
    }

    /// 处理滚动输入
    fn handle_scroll_input(&mut self, ui: &mut egui::Ui, content_rect: egui::Rect, row_height: f32, total_lines: usize) {
        let over_content = ui
            .input(|i| i.pointer.hover_pos())
            .map_or(false, |p| content_rect.contains(p));

        if !over_content {
            return;
        }

        let delta = ui.input(|i| i.smooth_scroll_delta.y);
        if delta.abs() < 0.01 {
            return;
        }

        let delta_lines = (-delta / row_height).round() as i32;
        if delta_lines != 0 {
            let new_line = (self.scroll.line as i32 + delta_lines).clamp(0, (total_lines - 1) as i32);
            self.scroll.line = new_line as usize;
        }
    }

    /// 渲染文本内容
    fn render_content(&mut self, ui: &mut egui::Ui, content_rect: egui::Rect, total_lines: usize) {
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(content_rect), |ui| {
            egui::ScrollArea::horizontal()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let start = self.scroll.line.min(total_lines - 1);
                    let end = (start + self.scroll.visible_lines).min(total_lines);

                    for line_num in start..end {
                        self.render_line(ui, line_num);
                    }
                });
        });
    }

    /// 渲染单行文本
    fn render_line(&mut self, ui: &mut egui::Ui, line_num: usize) {
        ui.horizontal(|ui| {
            // 行号
            if self.view.show_line_numbers {
                let ln = egui::RichText::new(format!("{:6} ", line_num + 1))
                    .monospace()
                    .color(egui::Color32::DARK_GRAY);
                ui.add(egui::Label::new(ln).selectable(false));
            }

            // 文本内容 - 从TextCache获取
            if let Some(line_text) = self.text_cache.get_line(line_num) {
                let text = line_text.trim_end_matches('\n').trim_end_matches('\r');
                
                // 检查是否有搜索结果需要高亮
                if !self.search.results.is_empty() && !self.search.query.is_empty() {
                    self.render_line_with_highlights(ui, text, line_num);
                } else {
                    ui.add(egui::Label::new(
                        egui::RichText::new(text)
                            .monospace()
                            .size(self.view.font_size)
                    ).extend());
                }
            } else {
                // 如果缓存中没有，显示空行
                ui.add(egui::Label::new(
                    egui::RichText::new("")
                        .monospace()
                        .size(self.view.font_size)
                ).extend());
            }
        });
    }

    /// 渲染带高亮的行
    fn render_line_with_highlights(&mut self, ui: &mut egui::Ui, line_text: &str, _line_num: usize) {
        // 使用搜索引擎在当前行中查找匹配
        let matches = self.search_engine.find_in_text(line_text);
        
        if matches.is_empty() {
            // 没有匹配，正常渲染
            ui.add(egui::Label::new(
                egui::RichText::new(line_text)
                    .monospace()
                    .size(self.view.font_size)
            ).extend());
            return;
        }

        // 构建带高亮的文本
        let mut job = egui::text::LayoutJob::default();
        let mut last_end = 0;

        for (start, end) in matches {
            // 添加匹配前的文本
            if start > last_end {
                job.append(
                    &line_text[last_end..start],
                    0.0,
                    egui::TextFormat {
                        font_id: egui::FontId::monospace(self.view.font_size),
                        color: egui::Color32::LIGHT_GRAY,
                        ..Default::default()
                    },
                );
            }

            // 添加高亮的匹配文本
            job.append(
                &line_text[start..end],
                0.0,
                egui::TextFormat {
                    font_id: egui::FontId::monospace(self.view.font_size),
                    color: egui::Color32::BLACK,
                    background: egui::Color32::YELLOW,
                    ..Default::default()
                },
            );

            last_end = end;
        }

        // 添加剩余文本
        if last_end < line_text.len() {
            job.append(
                &line_text[last_end..],
                0.0,
                egui::TextFormat {
                    font_id: egui::FontId::monospace(self.view.font_size),
                    color: egui::Color32::LIGHT_GRAY,
                    ..Default::default()
                },
            );
        }

        ui.add(egui::Label::new(job).extend());
    }
}

impl eframe::App for TextViewerAppSimp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 检测拖拽文件
        self.input_new_file(ctx);

        // 处理键盘快捷键
        self.handle_keyboard_shortcuts(ctx);

        // 轮询搜索结果
        self.poll_search_results();

        // 渲染UI
        self.render_menu_bar(ctx);
        self.render_search_bar(ctx);
        self.render_search_results_panel(ctx); // 添加搜索结果面板
        self.render_status_bar(ctx);
        self.render_text_area(ctx);

        // 保持搜索动画
        if self.search.in_progress {
            ctx.request_repaint();
        }
    }
}