#![allow(dead_code)]

// src/search_service.rs
use std::sync::{Arc, atomic::AtomicBool, mpsc};
use anyhow::Result;
use large_text_core::file_reader::FileReader;
use large_text_core::line_indexer::LineIndexer;
use large_text_core::search_engine::{SearchEngine, SearchMessage, SearchType};
use large_text_core::search_engine::SearchResult;
use large_text_core::text_cache::TextCache; //加速find_prev

/// 搜索结果项
#[derive(Debug, Clone)]
pub struct SearchMatch {
    pub line_number: usize,      // 行号（0-based）
    pub byte_offset: usize,      // 字节偏移量
    pub match_length: usize,     // 匹配长度
    pub line_text: String,       // 行文本内容
    //TODO 可以添加自动 il 分析
}

/// 搜索结果摘要
#[derive(Debug, Default)]
pub struct SearchSummary {
    pub total_matches: usize,
    pub matches: Vec<SearchMatch>,
    pub error: Option<String>,
}

/// 搜索配置
#[derive(Debug, Clone)]
pub struct SearchConfig {
    pub pattern: String,
    pub use_regex: bool,
    pub case_sensitive: bool,
    pub max_results: usize,
    pub context_lines: usize,
    pub line_start: Option<usize>,  // 1-based
    pub line_end: Option<usize>,    // 1-based
}

impl SearchConfig {
    pub fn new(pattern: String) -> Self {
        Self {
            pattern,
            use_regex: false,
            case_sensitive: false,
            max_results: 100,
            context_lines: 0,
            line_start: None,
            line_end: None,
        }
    }

    // 用于设置参数
    pub fn with_regex(mut self, use_regex: bool) -> Self {
        self.use_regex = use_regex;
        self
    }

    pub fn with_case_sensitive(mut self, case_sensitive: bool) -> Self {
        self.case_sensitive = case_sensitive;
        self
    }

    pub fn with_max_results(mut self, max_results: usize) -> Self {
        self.max_results = max_results;
        self
    }

    pub fn with_context(mut self, context_lines: usize) -> Self {
        self.context_lines = context_lines;
        self
    }

    pub fn with_line_range(mut self, start: Option<usize>, end: Option<usize>) -> Self {
        self.line_start = start;
        self.line_end = end;
        self
    }
}

/// 搜索服务
pub struct SearchService {
    reader: Arc<FileReader>,
    indexer: Arc<LineIndexer>,  // 改为 Arc
    text_cache: TextCache,
    search_engine: SearchEngine,  // 复用搜索引擎
}

impl SearchService {
    // 组合底层组件，有些api需要结合原子模块 才能使用
    pub fn new(reader: FileReader) -> Self {
        let reader = Arc::new(reader);

        let mut indexer = LineIndexer::new();
        indexer.index_file(&reader);
        let indexer = Arc::new(indexer);

        let mut text_cache = TextCache::default();
        text_cache.set_file(reader.clone(), indexer.clone());

        Self { 
            reader, 
            indexer,
            text_cache,
            search_engine: SearchEngine::new(),
        }
    }

    /// 获取文件阅读器引用
    pub fn reader(&self) -> Arc<FileReader> {
        self.reader.clone()
    }

    /// 获取行索引器引用
    pub fn indexer(&self) -> &LineIndexer { &self.indexer }

    /// 获取总行数
    pub fn total_lines(&self) -> usize { self.indexer.total_lines() }

    /// 获取指定行的文本（使用text_cache加速）
    pub fn get_line_text(&mut self, line_num: usize) -> Option<String> {
        self.text_cache.get_line(line_num)
            .map(|text| text.trim_end_matches('\n').trim_end_matches('\r').to_string())
    }

    /// 根据字节偏移量获取行号（使用二分查找优化）
    pub fn get_line_number(&self, byte_offset: usize) -> Option<usize> {
        // 直接使用 text_cache 的二分查找方法
        self.text_cache.get_line_info_by_offset(byte_offset)
            .map(|info| info.line_number)
    }

    /// 收集匹配行及其上下文的辅助方法
    fn collect_matches_with_context(
        &mut self,
        summary: &mut SearchSummary,
        last_line_shown: &mut Option<usize>,
        config: &SearchConfig,
        line_num: usize,
        result: &SearchResult,
    ) {
        // 优化：如果不需要上下文，只收集匹配行本身
        if config.context_lines == 0 {
            if let Some(line_text) = self.get_line_text(line_num) {
                summary.matches.push(SearchMatch {
                    line_number: line_num,
                    byte_offset: result.byte_offset,
                    match_length: result.match_len,
                    line_text,
                });
            }
            return;
        }
        
        // 计算需要显示的行范围（始终包含匹配行本身）
        let start_ctx = line_num.saturating_sub(config.context_lines);
        let end_ctx = (line_num + 1 + config.context_lines).min(self.total_lines());

        // 统一循环处理所有需要显示的行
        for ctx_line in start_ctx..end_ctx {
            if let Some(last) = *last_line_shown {
                if ctx_line <= last {
                    continue;
                }
            }

            if let Some(line_text) = self.get_line_text(ctx_line) {
                summary.matches.push(SearchMatch {
                    line_number: ctx_line,
                    byte_offset: result.byte_offset,
                    match_length: result.match_len,
                    line_text,
                });
                *last_line_shown = Some(ctx_line);
            }
        }
    }

    /// 执行搜索并返回结果摘要
    pub fn search(&mut self, config: SearchConfig) -> Result<SearchSummary> {
        // 复用 search_engine，只需重新设置查询
        self.search_engine.set_query(
            config.pattern.clone(),
            config.use_regex,
            config.case_sensitive,
        );

        let (tx, rx) = mpsc::sync_channel(1_000);
        let cancel_token = Arc::new(AtomicBool::new(false));

        // 计算行范围过滤器（转换为0-based）
        let line_filter = if config.line_start.is_some() || config.line_end.is_some() {
            let filter_start = config.line_start.map(|n| n.saturating_sub(1)).unwrap_or(0);
            let filter_end = config.line_end.map(|n| n.saturating_sub(1)).unwrap_or(usize::MAX);
            Some((filter_start, filter_end))
        } else {
            None
        };

        // 优化：将行号转换为字节偏移，让搜索引擎从指定位置开始
        let start_byte_offset = if let Some((filter_start, _)) = line_filter {
            self.indexer.get_line_range(filter_start)
                .map(|(start, _)| start)
                .unwrap_or(0)
        } else {
            0
        };

        // 启动搜索
        self.search_engine.fetch_matches(
            self.reader.clone(),
            tx,
            start_byte_offset,  // 从指定字节位置开始搜索
            config.max_results,
            cancel_token,
        );

        let mut summary = SearchSummary::default();
        let mut last_line_shown = None;

        loop {
            match rx.recv() {
                Ok(SearchMessage::ChunkResult(chunk)) => {
                    // 先匹配再取行
                    for result in chunk.matches {
                        if summary.matches.len() >= config.max_results {
                            break;
                        }

                        let Some(line_num) = self.get_line_number(result.byte_offset) else {
                            continue;
                        };

                        if let Some((filter_start, filter_end)) = line_filter {
                            if line_num < filter_start || line_num > filter_end {
                                continue;
                            }
                        }

                        self.collect_matches_with_context(
                            &mut summary,
                            &mut last_line_shown,
                            &config,
                            line_num,
                            &result,
                        );
                    }
                }
                Ok(SearchMessage::Done(SearchType::Fetch)) => break,
                Ok(SearchMessage::Error(e)) => {
                    summary.error = Some(e);
                    break;
                }
                _ => continue,
            }
        }

        summary.total_matches = summary.matches.len();
        Ok(summary)
    }

    /// 简单的计数搜索（只返回匹配数量）
    pub fn count_matches(&mut self, config: SearchConfig) -> Result<usize> {
        // 复用 search_engine
        self.search_engine.set_query(
            config.pattern,
            config.use_regex,
            config.case_sensitive,
        );

        let (tx, rx) = mpsc::sync_channel(10);
        let cancel_token = Arc::new(AtomicBool::new(false));
        // 启动了引擎后会自动取搜索
        self.search_engine.count_matches(self.reader.clone(), tx, cancel_token);

        let mut total = 0;
        loop {
            match rx.recv() {
                Ok(SearchMessage::CountResult(count)) => total += count,
                Ok(SearchMessage::Done(SearchType::Count)) => break,
                Ok(SearchMessage::Error(e)) => return Err(anyhow::anyhow!(e)),
                _ => continue,
            }
        }

        Ok(total)
    }

    /// 查找下一个匹配项（使用配置）
    pub fn find_next(&mut self, current_line: usize, config: SearchConfig) -> Option<SearchMatch> {
        // 确保从当前行的下一行开始搜索
        let mut search_config = config;
        search_config.line_start = Some(current_line + 1);  // 0-based，所以+1
        search_config.line_end = None;
        search_config.max_results = 1;  // 只需要第一个结果

        let summary = self.search(search_config).ok()?;
        summary.matches.into_iter().next()
    }

    /// 查找上一个匹配项（使用配置）
    /// 优化：使用小窗口反向搜索，避免全量扫描
    pub fn find_prev(&mut self, current_line: usize, config: SearchConfig) -> Option<SearchMatch> {
        if current_line == 0 {
            return None;
        }
        
        // 策略：从 current_line 向前搜索，使用小窗口
        // 窗口大小根据文件大小动态调整
        let window_size = 5000;
        let mut search_end = current_line;
        
        while search_end > 0 {
            let search_start = search_end.saturating_sub(window_size);
            
            let mut search_config = config.clone();
            search_config.line_start = Some(search_start + 1);  // 转换为 1-based
            search_config.line_end = Some(search_end);
            search_config.max_results = 5000;  // 放大窗口内结果数量以防止漏掉
            
            if let Ok(summary) = self.search(search_config) {
                // 因为返回的结果是从上到下的，所以我们需要反转或者取最后一个，其实我们要找最接近 current_line 的那一个
                // 但是对于 "st__" 这种大量的匹配，我们应该从下往上遍历所有结果
                // 注意：这里由于 search API 会返回 max_results 个结果，可能并不能包含所有的匹配项
                // 为了兼容基于 `SearchService` 的 `find_prev` 我们这里仅做简单返回最后一个
                if let Some(last_match) = summary.matches
                    .into_iter()
                    .filter(|m| m.line_number < current_line)
                    .last()
                {
                    return Some(last_match);
                }
            }
            
            // 没找到，继续向前搜索
            if search_start == 0 {
                break;
            }
            search_end = search_start;
        }
        
        None
    }
}

