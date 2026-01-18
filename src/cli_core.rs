use std::path::PathBuf;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}, mpsc};
use anyhow::Result;
use clap::{Parser, Subcommand};

use large_text_core::file_reader::{FileReader, detect_encoding};
use large_text_core::line_indexer::LineIndexer;
use large_text_core::text_cache::TextCache;
use large_text_core::search_engine::{SearchEngine, SearchMessage, SearchType};

#[derive(Parser)]
#[command(name = "large-text")]
#[command(about = "A high-performance text file processor")]
#[command(version = "1.0")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Display file information
    Info {
        /// Path to the text file
        #[arg(short, long)]
        file: PathBuf,
        /// Show encoding information
        #[arg(long)]
        encoding: bool,
    },
    /// Extract lines from file
    Lines {
        /// Path to the text file
        #[arg(short, long)]
        file: PathBuf,
        /// Start line number (1-based)
        #[arg(short, long, default_value = "1")]
        start: usize,
        /// End line number (1-based, optional)
        #[arg(short, long)]
        end: Option<usize>,
        /// Number of lines to show (alternative to end)
        #[arg(short, long)]
        count: Option<usize>,
        /// Show line numbers
        #[arg(long)]
        line_numbers: bool,
    },
    /// Search text in file
    Search {
        /// Path to the text file
        #[arg(short, long)]
        file: PathBuf,
        /// Search pattern
        #[arg(short, long)]
        pattern: String,
        /// Use regex pattern
        #[arg(long)]
        regex: bool,
        /// Case sensitive search
        #[arg(long)]
        case_sensitive: bool,
        /// Only count matches
        #[arg(long)]
        count_only: bool,
        /// Maximum number of results to show
        #[arg(long, default_value = "100")]
        max_results: usize,
        /// Show context lines around matches
        #[arg(short, long, default_value = "0")]
        context: usize,
    },
    /// Get specific byte range from file
    Bytes {
        /// Path to the text file
        #[arg(short, long)]
        file: PathBuf,
        /// Start byte offset
        #[arg(short, long)]
        start: usize,
        /// End byte offset
        #[arg(short, long)]
        end: usize,
    },
}

pub struct CliProcessor {
    text_cache: TextCache,
}

impl Default for CliProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl CliProcessor {
    pub fn new() -> Self {
        Self {
            text_cache: TextCache::new(10000), // 更大的缓存用于CLI操作
        }
    }

    /// 处理CLI命令
    pub fn process_command(&mut self, cli: Cli) -> Result<()> {
        match cli.command {
            Commands::Info { file, encoding } => {
                self.handle_info(file, encoding)
            }
            Commands::Lines { file, start, end, count, line_numbers } => {
                self.handle_lines(file, start, end, count, line_numbers)
            }
            Commands::Search { file, pattern, regex, case_sensitive, count_only, max_results, context } => {
                self.handle_search(file, pattern, regex, case_sensitive, count_only, max_results, context)
            }
            Commands::Bytes { file, start, end } => {
                self.handle_bytes(file, start, end)
            }
        }
    }

    /// 处理文件信息命令
    fn handle_info(&mut self, file_path: PathBuf, show_encoding: bool) -> Result<()> {
        let reader = FileReader::new(file_path.clone(), encoding_rs::UTF_8)?;
        let mut indexer = LineIndexer::new();
        indexer.index_file(&reader);
        
        self.text_cache.set_file(Arc::new(reader), indexer);
        
        println!("File: {}", file_path.display());
        println!("Size: {} bytes", self.text_cache.file_size());
        println!("Lines: {}", self.text_cache.total_lines());
        
        if show_encoding {
            // 检测编码
            let sample_bytes = self.text_cache.get_file_reader()
                .map(|r| r.get_bytes(0, 1024.min(r.len())))
                .unwrap_or(&[]);
            let detected_encoding = detect_encoding(sample_bytes);
            println!("Detected encoding: {}", detected_encoding.name());
        }
        
        Ok(())
    }

    /// 处理行提取命令
    fn handle_lines(&mut self, file_path: PathBuf, start: usize, end: Option<usize>, count: Option<usize>, show_line_numbers: bool) -> Result<()> {
        let reader = FileReader::new(file_path, encoding_rs::UTF_8)?;
        let mut indexer = LineIndexer::new();
        indexer.index_file(&reader);
        
        self.text_cache.set_file(Arc::new(reader), indexer);
        
        let total_lines = self.text_cache.total_lines();
        let start_line = start.saturating_sub(1); // 转换为0-based
        
        let end_line = if let Some(end) = end {
            end.min(total_lines)
        } else if let Some(count) = count {
            (start_line + count).min(total_lines)
        } else {
            total_lines
        };
        
        if start_line >= total_lines {
            println!("Start line {} exceeds file length ({} lines)", start, total_lines);
            return Ok(());
        }
        
        for line_num in start_line..end_line {
            if let Some(line_text) = self.text_cache.get_line(line_num) {
                let clean_text = line_text.trim_end_matches('\n').trim_end_matches('\r');
                if show_line_numbers {
                    println!("{:6}: {}", line_num + 1, clean_text);
                } else {
                    println!("{}", clean_text);
                }
            }
        }
        
        Ok(())
    }

    /// 处理搜索命令
    fn handle_search(&mut self, file_path: PathBuf, pattern: String, use_regex: bool, case_sensitive: bool, count_only: bool, max_results: usize, context: usize) -> Result<()> {
        let reader = Arc::new(FileReader::new(file_path, encoding_rs::UTF_8)?);
        let mut indexer = LineIndexer::new();
        indexer.index_file(&reader);
        
        self.text_cache.set_file(reader.clone(), indexer);
        
        let mut search_engine = SearchEngine::new();
        search_engine.set_query(pattern.clone(), use_regex, case_sensitive);
        
        let (tx, rx) = mpsc::sync_channel(10_000);
        let cancel_token = Arc::new(AtomicBool::new(false));
        
        if count_only {
            // 只计数
            search_engine.count_matches(reader, tx, cancel_token);
            
            let mut total_count = 0;
            loop {
                match rx.recv() {
                    // 匹配信息
                    Ok(SearchMessage::CountResult(count)) => {
                        total_count += count;
                    }
                    Ok(SearchMessage::Done(SearchType::Count)) => break,
                    Ok(SearchMessage::Error(e)) => {
                        eprintln!("Search error: {}", e);
                        return Ok(());
                    }
                    _ => continue,
                }
            }
            
            println!("Found {} matches for pattern: {}", total_count, pattern);
        } else {
            // 获取匹配结果
            search_engine.fetch_matches(reader.clone(), tx, 0, max_results, cancel_token);
            
            let mut results_shown = 0;
            loop {
                match rx.recv() {
                    Ok(SearchMessage::ChunkResult(chunk)) => {
                        for result in chunk.matches {
                            if results_shown >= max_results {
                                break;
                            }
                            
                            // 找到匹配所在的行
                            if let Some(line_info) = self.text_cache.get_line_info_by_offset(result.byte_offset) {
                                let line_num = line_info.line_number;
                                
                                // 显示上下文
                                let start_context = line_num.saturating_sub(context);
                                let end_context = (line_num + context + 1).min(self.text_cache.total_lines());
                                
                                if context > 0 && results_shown > 0 {
                                    println!("--");
                                }
                                
                                for ctx_line in start_context..end_context {
                                    if let Some(line_text) = self.text_cache.get_line(ctx_line) {
                                        let clean_text = line_text.trim_end_matches('\n').trim_end_matches('\r');
                                        let prefix = if ctx_line == line_num { ">" } else { " " };
                                        println!("{} {:6}: {}", prefix, ctx_line + 1, clean_text);
                                    }
                                }
                                // }
                            }
                            
                            results_shown += 1;
                        }
                    }
                    Ok(SearchMessage::Done(SearchType::Fetch)) => break,
                    Ok(SearchMessage::Error(e)) => {
                        eprintln!("Search error: {}", e);
                        return Ok(());
                    }
                    _ => continue,
                }
            }
            
            if results_shown == 0 {
                println!("No matches found for pattern: {}", pattern);
            } else {
                println!("\nShowed {} matches", results_shown);
            }
        }
        
        Ok(())
    }

    /// 处理字节范围命令
    fn handle_bytes(&mut self, file_path: PathBuf, start: usize, end: usize) -> Result<()> {
        let reader = FileReader::new(file_path, encoding_rs::UTF_8)?;
        
        if start >= reader.len() {
            println!("Start offset {} exceeds file size ({} bytes)", start, reader.len());
            return Ok(());
        }
        
        let actual_end = end.min(reader.len());
        let chunk = reader.get_chunk(start, actual_end);
        
        println!("Bytes {}..{} ({} bytes):", start, actual_end, actual_end - start);
        println!("{}", chunk);
        
        Ok(())
    }
}

/// CLI入口函数
pub fn run_cli() -> Result<()> {
    let cli = Cli::parse();
    let mut processor = CliProcessor::new();
    processor.process_command(cli)
}