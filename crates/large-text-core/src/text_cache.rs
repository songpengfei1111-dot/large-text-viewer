use std::sync::Arc;
use crate::file_reader::FileReader;
use crate::line_indexer::LineIndexer;

/// 智能文本缓存，自动管理chunk更新和reader交互
pub struct TextCache {
    cached_text: Option<String>,
    cached_start_line: usize,
    cached_end_line: usize,
    cached_start_offset: usize,
    cache_size: usize, // 缓存大小（行数）
    reader: Option<Arc<FileReader>>,
    indexer: Option<LineIndexer>,
}

impl Default for TextCache {
    fn default() -> Self {
        Self {
            cached_text: None,
            cached_start_line: 0,
            cached_end_line: 0,
            cached_start_offset: 0,
            cache_size: 1000, // 默认缓存1000行
            reader: None,
            indexer: None,
        }
    }
}

impl TextCache {
    pub fn new(cache_size: usize) -> Self {
        Self {
            cache_size,
            ..Default::default()
        }
    }

    /// 设置reader和indexer
    pub fn set_file(&mut self, reader: Arc<FileReader>, indexer: LineIndexer) {
        self.reader = Some(reader);
        self.indexer = Some(indexer);
        self.clear_cache();
    }

    /// 获取总行数
    pub fn total_lines(&self) -> usize {
        self.indexer.as_ref().map_or(0, |idx| idx.total_lines())
    }

    /// 获取文件路径
    pub fn file_path(&self) -> Option<&std::path::Path> {
        self.reader.as_ref().map(|r| r.path().as_path())
    }

    /// 智能获取行文本，自动更新缓存
    pub fn get_line(&mut self, line_num: usize) -> Option<String> {
        // 检查是否有必要的组件
        if self.reader.is_none() || self.indexer.is_none() {
            return None;
        }

        // 检查是否需要更新缓存
        if self.needs_update(line_num) {
            self.update_cache_around_line(line_num);
        }

        // 从缓存获取
        self.get_line_from_cache(line_num)
    }

    /// 获取行范围的文本
    pub fn get_lines(&mut self, start_line: usize, end_line: usize) -> Vec<String> {
        let mut result = Vec::new();
        
        // 检查是否有必要的组件
        if self.reader.is_none() || self.indexer.is_none() {
            return result;
        }

        let total_lines = self.indexer.as_ref().unwrap().total_lines();

        // 检查是否需要更新缓存
        if self.needs_update_for_range(start_line, end_line, total_lines) {
            self.update_cache_for_range(start_line, end_line);
        }

        // 从缓存获取
        for line_num in start_line..end_line {
            if let Some(line_text) = self.get_line_from_cache(line_num) {
                result.push(line_text);
            } else {
                result.push(String::new());
            }
        }
        
        result
    }

    /// 检查是否需要更新缓存
    fn needs_update(&self, line_num: usize) -> bool {
        if self.cached_text.is_none() {
            return true;
        }

        // 如果请求的行不在缓存范围内
        if line_num < self.cached_start_line || line_num >= self.cached_end_line {
            return true;
        }

        // 如果接近缓存边界，预加载
        let buffer_zone = self.cache_size / 4;
        line_num < self.cached_start_line + buffer_zone || 
        line_num > self.cached_end_line.saturating_sub(buffer_zone)
    }

    /// 检查范围是否需要更新缓存
    fn needs_update_for_range(&self, start_line: usize, end_line: usize, _total_lines: usize) -> bool {
        if self.cached_text.is_none() {
            return true;
        }

        start_line < self.cached_start_line || end_line > self.cached_end_line
    }

    /// 围绕指定行更新缓存
    fn update_cache_around_line(&mut self, center_line: usize) {
        // 先获取所有需要的引用
        let (reader, indexer) = match (&self.reader, &self.indexer) {
            (Some(r), Some(i)) => (r.clone(), i),
            _ => return,
        };

        let total_lines = indexer.total_lines();
        let half_cache = self.cache_size / 2;
        
        let start_line = center_line.saturating_sub(half_cache);
        let end_line = (center_line + half_cache).min(total_lines);
        
        // 内联update_cache逻辑
        if let Some((chunk_start_offset, chunk_end_offset)) = get_chunk_range(start_line, end_line, indexer) {
            self.cached_text = Some(reader.get_chunk(chunk_start_offset, chunk_end_offset));
            self.cached_start_line = start_line;
            self.cached_end_line = end_line;
            self.cached_start_offset = chunk_start_offset;
        }
    }

    /// 为指定范围更新缓存
    fn update_cache_for_range(&mut self, start_line: usize, end_line: usize) {
        // 先获取所有需要的引用
        let (reader, indexer) = match (&self.reader, &self.indexer) {
            (Some(r), Some(i)) => (r.clone(), i),
            _ => return,
        };

        let total_lines = indexer.total_lines();
        let range_size = end_line - start_line;
        let extra_buffer = (self.cache_size.saturating_sub(range_size)) / 2;
        
        let cache_start = start_line.saturating_sub(extra_buffer);
        let cache_end = (end_line + extra_buffer).min(total_lines);
        
        // 内联update_cache逻辑
        if let Some((chunk_start_offset, chunk_end_offset)) = get_chunk_range(cache_start, cache_end, indexer) {
            self.cached_text = Some(reader.get_chunk(chunk_start_offset, chunk_end_offset));
            self.cached_start_line = cache_start;
            self.cached_end_line = cache_end;
            self.cached_start_offset = chunk_start_offset;
        }
    }

    /// 更新缓存（现在不再使用，保留以防需要）
    #[allow(dead_code)]
    fn update_cache(&mut self, start_line: usize, end_line: usize, reader: &Arc<FileReader>, indexer: &LineIndexer) {
        if let Some((chunk_start_offset, chunk_end_offset)) = get_chunk_range(start_line, end_line, indexer) {
            self.cached_text = Some(reader.get_chunk(chunk_start_offset, chunk_end_offset));
            self.cached_start_line = start_line;
            self.cached_end_line = end_line;
            self.cached_start_offset = chunk_start_offset;
        }
    }

    /// 从缓存获取行文本
    fn get_line_from_cache(&self, line_num: usize) -> Option<String> {
        let cached_text = self.cached_text.as_ref()?;
        let indexer = self.indexer.as_ref()?;
        
        if line_num < self.cached_start_line || line_num >= self.cached_end_line {
            return None;
        }

        let (line_start_offset, line_end_offset) = indexer.get_line_range(line_num)?;
        let relative_start = line_start_offset.saturating_sub(self.cached_start_offset);
        let relative_end = if line_end_offset == usize::MAX {
            cached_text.len()
        } else {
            (line_end_offset.saturating_sub(self.cached_start_offset)).min(cached_text.len())
        };

        if relative_start >= cached_text.len() || relative_start >= relative_end {
            return None;
        }

        if cached_text.is_char_boundary(relative_start) && cached_text.is_char_boundary(relative_end) {
            Some(cached_text[relative_start..relative_end].to_string())
        } else {
            None
        }
    }

    /// 清空缓存
    pub fn clear_cache(&mut self) {
        self.cached_text = None;
        self.cached_start_line = 0;
        self.cached_end_line = 0;
        self.cached_start_offset = 0;
    }

    /// 完全清空，包括reader和indexer
    pub fn clear(&mut self) {
        self.clear_cache();
        self.reader = None;
        self.indexer = None;
    }

    /// 获取文件大小
    pub fn file_size(&self) -> usize {
        self.reader.as_ref().map_or(0, |r| r.len())
    }

    /// 获取FileReader引用（用于CLI操作）
    pub fn get_file_reader(&self) -> Option<&Arc<FileReader>> {
        self.reader.as_ref()
    }

    /// 根据字节偏移量获取行信息
    pub fn get_line_info_by_offset(&self, byte_offset: usize) -> Option<LineInfo> {
        let indexer = self.indexer.as_ref()?;
        
        // 遍历所有行找到包含该偏移量的行
        for line_num in 0..indexer.total_lines() {
            if let Some((start_offset, end_offset)) = indexer.get_line_range(line_num) {
                if byte_offset >= start_offset && (end_offset == usize::MAX || byte_offset < end_offset) {
                    return Some(LineInfo {
                        line_number: line_num,
                        start_offset,
                        end_offset,
                    });
                }
            }
        }
        None
    }
}

/// 行信息结构体
#[derive(Debug, Clone)]
pub struct LineInfo {
    pub line_number: usize,
    pub start_offset: usize,
    pub end_offset: usize,
}


/// 获取chunk的字节范围
fn get_chunk_range(start_line: usize, end_line: usize, indexer: &LineIndexer) -> Option<(usize, usize)> {
    if start_line >= indexer.total_lines() {
        return None;
    }

    let chunk_start = indexer.get_line_range(start_line)?.0;
    let chunk_end = if end_line >= indexer.total_lines() {
        usize::MAX
    } else {
        indexer.get_line_range(end_line)?.0
    };

    Some((chunk_start, chunk_end))
}