#![allow(dead_code)]

// src/search_service.rs
use std::sync::{Arc, atomic::AtomicBool, mpsc};
use anyhow::Result;
use large_text_core::file_reader::FileReader;
use large_text_core::line_indexer::LineIndexer;
use large_text_core::search_engine::{SearchEngine, SearchMessage, SearchType};
use large_text_core::search_engine::SearchResult;

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
    indexer: LineIndexer,
}

impl SearchService {
    // 组合底层组间，有些api需要结合原子模块 才能使用
    pub fn new(reader: FileReader) -> Self {
        let reader = Arc::new(reader);
        let mut indexer = LineIndexer::new();
        indexer.index_file(&reader);

        Self { reader, indexer }
    }

    /// 获取文件阅读器引用
    pub fn reader(&self) -> Arc<FileReader> {
        self.reader.clone()
    }

    /// 获取行索引器引用
    pub fn indexer(&self) -> &LineIndexer {
        &self.indexer
    }

    /// 获取总行数
    pub fn total_lines(&self) -> usize {
        self.indexer.total_lines()
    }

    /// 获取指定行的文本
    pub fn get_line_text(&self, line_num: usize) -> Option<String> {
        if let Some((start, end)) = self.indexer.get_line_range(line_num) {
            let text = if end == usize::MAX {
                self.reader.get_chunk(start, self.reader.len())
            } else {
                self.reader.get_chunk(start, end)
            };

            Some(text.trim_end_matches('\n').trim_end_matches('\r').to_string())
        } else {
            None
        }
    }

    /// 根据字节偏移量获取行号
    pub fn get_line_number(&self, byte_offset: usize) -> Option<usize> {
        for line_num in 0..self.indexer.total_lines() {
            if let Some((start, end)) = self.indexer.get_line_range(line_num) {
                if byte_offset >= start && (end == usize::MAX || byte_offset < end) {
                    return Some(line_num);
                }
            }
        }
        None
    }

    /// 收集匹配行及其上下文的辅助方法
    fn collect_matches_with_context(
        &self,
        summary: &mut SearchSummary,
        last_line_shown: &mut Option<usize>,
        config: &SearchConfig,
        line_num: usize,
        result: &SearchResult,
    ) {
        // 计算需要显示的行范围（始终包含匹配行本身）
        let start_ctx = if config.context_lines > 0 {
            line_num.saturating_sub(config.context_lines)
        } else {
            line_num  // 没有上下文时只显示匹配行
        };

        let end_ctx = if config.context_lines > 0 {
            (line_num + 1 + config.context_lines).min(self.total_lines())
        } else {
            line_num + 1  // 只显示匹配行
        };

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
    pub fn search(&self, config: SearchConfig) -> Result<SearchSummary> {
        let mut search_engine = SearchEngine::new(); //修改到类里，不要每次都新建
        search_engine.set_query(
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

        // 启动搜索
        search_engine.fetch_matches(
            self.reader.clone(),
            tx,
            0,
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
    pub fn count_matches(&self, config: SearchConfig) -> Result<usize> {
        let mut search_engine = SearchEngine::new();
        search_engine.set_query(
            config.pattern,
            config.use_regex,
            config.case_sensitive,
        );

        let (tx, rx) = mpsc::sync_channel(10);
        let cancel_token = Arc::new(AtomicBool::new(false));
        // 启动了引擎后会自动取搜索
        search_engine.count_matches(self.reader.clone(), tx, cancel_token);

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
    pub fn find_next(&self, current_line: usize, config: SearchConfig) -> Option<SearchMatch> {
        // 确保从当前行的下一行开始搜索
        let mut search_config = config;
        search_config.line_start = Some(current_line + 1);  // 0-based，所以+1
        search_config.line_end = None;
        search_config.max_results = 1;  // 只需要第一个结果

        let summary = self.search(search_config).ok()?;
        summary.matches.into_iter().next()
    }

    /// 查找上一个匹配项（使用配置）
    /// 还是使用text_cache,要不反复扫太慢了
    pub fn find_prev(&self, current_line: usize, config: SearchConfig) -> Option<SearchMatch> {
        // 获取当前行之前的所有结果
        println!("[debug] {}", current_line);
        let mut search_config = config;
        search_config.line_end = Some(current_line);  // 只搜索当前行之前
        search_config.max_results = usize::MAX;  // 获取所有结果以便找到最后一个

        let summary = self.search(search_config).ok()?;

        // 找到最后一个小于当前行号的匹配项
        summary.matches
            .into_iter()
            .filter(|m| m.line_number < current_line)
            .last()
    }
}

