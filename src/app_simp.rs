use eframe::egui;
use std::path::PathBuf;
use std::sync::Arc;

use large_text_core::file_reader::FileReader;
use large_text_core::line_indexer::LineIndexer;
use large_text_core::text_cache::TextCache;


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
        font_size: f32,
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
        self.render_text(ui, rect, start_line, end_line, current_line, visible_lines, font_size, text_cache);

        // 处理点击
        self.handle_click(ui, rect, start_line, end_line, total_lines)
    }

    /// 渲染文本内容
    /// 渲染文本内容
    fn render_text(
        &self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        start_line: usize,
        end_line: usize,
        current_line: usize,
        visible_lines: usize,
        font_size: f32,
        text_cache: &mut TextCache,
    ) {
        let available_height = rect.height() - 20.0;
        let actual_range = end_line - start_line;
        let line_height = available_height / actual_range as f32;
        let mini_font_size = (font_size * 0.3).max(4.0);

        // 批量获取文本行
        let lines = text_cache.get_lines(start_line, end_line);

        // 创建用于文本显示的滚动区域
        egui::ScrollArea::vertical()
            .max_height(available_height)
            .show(ui, |ui| {
                // 设置字体
                ui.style_mut().text_styles.insert(
                    egui::TextStyle::Body,
                    egui::FontId::monospace(mini_font_size)
                );

                // 创建垂直布局
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing = egui::Vec2::new(0.0, 2.0);

                    for (idx, line_text) in lines.iter().enumerate() {
                        let line_num = start_line + idx;

                        // 处理文本
                        let processed_text = line_text
                            .split('\n').next().unwrap_or("")
                            .trim_end_matches('\r')
                            .chars()
                            .take(50)
                            .collect::<String>();

                        if processed_text.is_empty() {
                            ui.label(""); // 空行
                            continue;
                        }

                        // 判断是否在视口中
                        let is_in_viewport = line_num >= current_line && line_num < current_line + visible_lines;

                        // 创建富文本
                        let text_color = if is_in_viewport {
                            egui::Color32::from_gray(200)
                        } else {
                            egui::Color32::from_gray(150)
                        };

                        // 使用富文本渲染
                        let rich_text = egui::RichText::new(processed_text)
                            .family(egui::FontFamily::Monospace)
                            .size(mini_font_size)
                            .color(text_color);

                        // 渲染文本行
                        ui.label(rich_text);
                    }
                });
            });

        // 绘制当前视口高亮
        let viewport_start_relative = current_line.saturating_sub(start_line);
        let viewport_end_relative = (current_line + visible_lines).saturating_sub(start_line);

        let viewport_top = rect.top() + 10.0 + viewport_start_relative as f32 * line_height;
        let viewport_bottom = rect.top() + 10.0 + viewport_end_relative as f32 * line_height;
        let viewport_height = (viewport_bottom - viewport_top).max(8.0);

        let viewport_rect = egui::Rect::from_min_size(
            egui::pos2(rect.left() + 2.0, viewport_top),
            egui::vec2(rect.width() - 4.0, viewport_height),
        );

        let painter = ui.painter();
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
    minimap: MiniMap,
    scroll: ScrollState,
    view: ViewSettings,
    status_message: String,
}

impl Default for TextViewerAppSimp {
    fn default() -> Self {
        Self {
            text_cache: TextCache::new(2000), // 缓存2000行
            minimap: MiniMap::default(),
            scroll: ScrollState::default(),
            view: ViewSettings::default(),
            status_message: String::new(),
        }
    }
}

impl TextViewerAppSimp {
    /// 打开文件
    fn open_file(&mut self, path: PathBuf) {
        match FileReader::new(path.clone(), encoding_rs::UTF_8) {
            Ok(reader) => {
                let mut indexer = LineIndexer::new();
                indexer.index_file(&reader);
                
                self.text_cache.set_file(Arc::new(reader), indexer);
                self.scroll.reset();
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
            });
        });
    }

    /// 渲染状态栏
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
                self.view.font_size,
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
                ui.add(egui::Label::new(
                    egui::RichText::new(text)
                        .monospace()
                        .size(self.view.font_size)
                ).extend());
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
}

impl eframe::App for TextViewerAppSimp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.render_menu_bar(ctx);
        self.render_status_bar(ctx);
        self.render_text_area(ctx);
    }
}