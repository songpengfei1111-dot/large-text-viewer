#[warn(dead_code)]

use std::path::PathBuf;
use anyhow::Result;
use clap::{Parser, Subcommand};
use large_text_core::file_reader::{FileReader, detect_encoding};
// 移除 mod search_service;，只保留 use
use crate::search_service::{SearchService, SearchConfig};

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
        /// Maximum number of results to show
        #[arg(long, default_value = "100")]
        max_results: usize,
        /// Show context lines around matches
        #[arg(short, long, default_value = "0")]
        context: usize,
        /// Start line number for filtering results (1-based, optional)
        #[arg(long)]
        start: Option<usize>,
        /// End line number for filtering results (1-based, optional)
        #[arg(long)]
        end: Option<usize>,
        /// Only count matches, don't show content
        #[arg(long)]
        count_only: bool,
    },
    FindNext {
        /// Path to the text file
        #[arg(short, long)]
        file: PathBuf,
        /// Search pattern
        #[arg(short, long)]
        pattern: String,
        /// Current line number (1-based)
        #[arg(short, long)]
        line: usize,
        /// Search direction: 0 for previous (up), 1 for next (down) [default: 1]
        #[arg(short, long, default_value = "1")]
        direction: u8,
        /// Use regex pattern
        #[arg(long)]
        regex: bool,
        /// Show context lines around match
        #[arg(short, long, default_value = "0")]
        context: usize,
    },



}

pub struct CliProcessor;

impl Default for CliProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl CliProcessor {
    /// TODO 添加污点接口
    /// 添加原生python交互
    pub fn new() -> Self {
        Self
    }

    /// 处理CLI命令
    pub fn process_command(&self, cli: Cli) -> Result<()> {
        match cli.command {
            Commands::Info { file, encoding } => {
                self.handle_info(file, encoding)
            }
            Commands::Lines { file, start, end, count, line_numbers } => {
                self.handle_lines(file, start, end, count, line_numbers)
            }
            Commands::Search { file, pattern, regex, max_results, context, start, end, count_only } => {
                self.handle_search(file, pattern, regex, max_results, context, start, end, count_only)
            }
            Commands::FindNext { file, pattern, line, direction, regex, context } => {
                self.handle_find(file, pattern, line, direction, regex, context)
            }
        }
    }

    /// 处理文件信息命令
    fn handle_info(&self, file_path: PathBuf, show_encoding: bool) -> Result<()> {
        let reader = FileReader::new(file_path.clone(), encoding_rs::UTF_8)?;
        let service = SearchService::new(reader);

        println!("File: {}", file_path.display());
        println!("Size: {} bytes", service.reader().len());
        println!("Lines: {}", service.total_lines());

        if show_encoding {
            // 修复临时值问题：先获取 reader 的引用
            let reader_ref = service.reader();
            let sample_bytes = reader_ref.get_bytes(0, 1024.min(reader_ref.len()));
            let detected_encoding = detect_encoding(sample_bytes);
            println!("Detected encoding: {}", detected_encoding.name());
        }

        Ok(())
    }

    /// 处理行提取命令
    fn handle_lines(&self, file_path: PathBuf, start: usize, end: Option<usize>, count: Option<usize>, show_line_numbers: bool) -> Result<()> {
        let reader = FileReader::new(file_path, encoding_rs::UTF_8)?;
        let mut service = SearchService::new(reader);

        let total_lines = service.total_lines();
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
            if let Some(line_text) = service.get_line_text(line_num) {
                if show_line_numbers {
                    println!("{:6}: {}", line_num + 1, line_text);
                } else {
                    println!("{}", line_text);
                }
            }
        }

        Ok(())
    }

    /// 处理搜索命令
    fn handle_search(&self, file_path: PathBuf, pattern: String, use_regex: bool, max_results: usize, context: usize, start: Option<usize>, end: Option<usize>, count_only: bool) -> Result<()> {
        let reader = FileReader::new(file_path, encoding_rs::UTF_8)?;
        let mut service = SearchService::new(reader);

        let config = SearchConfig::new(pattern.clone())
            .with_regex(use_regex)
            .with_max_results(max_results)
            .with_context(context)
            .with_line_range(start, end);

        if count_only {
            // 只计数模式
            let count = service.count_matches(config)?;
            println!("Total matches: {}", count);
            return Ok(());
        }

        // 正常搜索模式
        let summary = service.search(config)?;

        if summary.matches.is_empty() {
            println!("No matches found for pattern: {}", pattern);
            return Ok(());
        }

        let mut last_line = None;

        for m in &summary.matches {
            // 如果是新的匹配组，添加分隔线
            if let Some(last) = last_line {
                if m.line_number > last + 1 {
                    println!("...");
                }
            }

            //在视觉上区分context行和搜索结果
            let prefix = if context > 0 && m.line_number == service.get_line_number(m.byte_offset).unwrap_or(0) {
                ">"
            } else {
                " "
            };

            println!("{}{:6}: {}", prefix, m.line_number + 1, m.line_text);
            last_line = Some(m.line_number);
        }

        println!("\nShowed {} matches", summary.total_matches);

        Ok(())
    }


    // 添加统一的处理方法
    /// 处理 find 命令
    fn handle_find(&self, file_path: PathBuf, pattern: String, line: usize, direction: u8, use_regex: bool, context: usize) -> Result<()> {
        let reader = FileReader::new(file_path, encoding_rs::UTF_8)?;
        let mut service = SearchService::new(reader);

        // 转换为0-based行号
        let current_line = line.saturating_sub(1);

        // 创建统一的配置
        let config = SearchConfig::new(pattern.clone())
            .with_regex(use_regex)
            .with_context(context)  // 上下文行数也会被使用
            .with_case_sensitive(false);  // 默认不区分大小写

        // 根据方向使用对应的查找方法
        let result = if direction == 0 {
            service.find_prev(current_line, config)
        } else {
            service.find_next(current_line, config)
        };

        match result {
            Some(m) => {
                let direction_str = if direction == 0 { "Previous" } else { "Next" };
                println!("{} match found at line {}:", direction_str, m.line_number + 1);

                // 显示上下文（从config中获取）
                if context > 0 {
                    let start_ctx = if m.line_number > context {
                        m.line_number - context
                    } else {
                        0
                    };
                    let end_ctx = (m.line_number + 1 + context).min(service.total_lines());

                    for ctx_line in start_ctx..end_ctx {
                        if let Some(ctx_text) = service.get_line_text(ctx_line) {
                            let prefix = if ctx_line == m.line_number { ">" } else { " " };
                            println!("{}{:6}: {}", prefix, ctx_line + 1, ctx_text);
                        }
                    }
                } else {
                    println!("{:6}: {}", m.line_number + 1, m.line_text);
                }
            }
            None => {
                let direction_str = if direction == 0 { "previous" } else { "next" };
                println!("No {} match found for pattern: {}", direction_str, pattern);
            }
        }

        Ok(())
    }
}

// 其他的功能函数也在这里管理

/// CLI入口函数
pub fn run_cli() -> Result<()> {
    let cli = Cli::parse();
    let processor = CliProcessor::new();
    processor.process_command(cli)
}