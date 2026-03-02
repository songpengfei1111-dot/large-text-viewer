// taint_engine.rs
use crate::search_service::{SearchService, SearchConfig, SearchMatch};
use anyhow::Result;
use std::collections::HashSet;

pub struct TaintEngine {
    service: SearchService,
    max_depth: usize,
    visited: HashSet<usize>,
    verbose: bool,
}

#[derive(Debug, Clone)]
pub struct TracePath {
    pub line_num: usize,
    pub instruction: String,
    pub trace_type: TraceType,
    pub depth: usize,
    pub sources: Vec<TracePath>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TraceType {
    MemToReg,
    RegToMem,
    RegToReg,
    Arith,
    Constant,
    Unknown,
    End,
}

impl std::fmt::Display for TraceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TraceType::MemToReg => write!(f, "MEM→REG"),
            TraceType::RegToMem => write!(f, "REG→MEM"),
            TraceType::RegToReg => write!(f, "REG→REG"),
            TraceType::Arith => write!(f, "ARITH"),
            TraceType::Constant => write!(f, "CONST"),
            TraceType::Unknown => write!(f, "UNKNOWN"),
            TraceType::End => write!(f, "END"),
        }
    }
}

// 指令类型枚举
#[derive(Debug)]
enum InstructionType {
    MemoryRead { addr: String },
    MemoryWrite { reg: String, value: String },
    RegTransfer { reg: String, value: String },
    Arithmetic { regs: Vec<String> },
    Other,
}

impl TaintEngine {
    pub fn new(service: SearchService) -> Self {
        Self {
            service,
            max_depth: 10,
            visited: HashSet::new(),
            verbose: true,
        }
    }

    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = depth;
        self
    }

    pub fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    /// 主入口：从指定行开始反向追踪
    pub fn trace_backward(&mut self, start_line: usize, target: &str) -> Result<Option<TracePath>> {
        self.visited.clear();
        self.log(&format!("\n=== 开始反向追踪: {} 从行{} ===\n", target, start_line + 1));
        Ok(self.trace_backward_internal(start_line, target, 0))
    }

    fn trace_backward_internal(&mut self, line_num: usize, target: &str, depth: usize) -> Option<TracePath> {
        if depth >= self.max_depth || self.visited.contains(&line_num) {
            return None;
        }
        self.visited.insert(line_num);

        let line_text = self.service.get_line_text(line_num)?;

        let mut current = TracePath {
            line_num,
            instruction: line_text.clone(),
            trace_type: TraceType::Unknown,
            depth,
            sources: vec![],
        };

        match self.parse_instruction_type(&line_text) {
            InstructionType::MemoryRead { addr } => {
                self.handle_memory_read(&mut current, line_num, &addr, depth)?;
            }
            InstructionType::MemoryWrite { reg, value } => {
                self.handle_memory_write(&mut current, line_num, &reg, &value, depth)?;
            }
            InstructionType::RegTransfer { reg, value } => {
                self.handle_reg_transfer(&mut current, line_num, &value, depth)?;
            }
            InstructionType::Arithmetic { regs } => {
                self.handle_arithmetic(&mut current, line_num, regs, depth)?;
            }
            InstructionType::Other => {
                current.trace_type = TraceType::Constant;
                self.log("终点/常量");
            }
        }

        Some(current)
    }

    // 解析指令类型
    fn parse_instruction_type(&self, line: &str) -> InstructionType {
        if line.contains("ld__") {
            if let Some(addr) = line.split(';')
                .find(|p| p.contains("ld__"))
                .and_then(|p| p.split('_').nth(1)) {
                return InstructionType::MemoryRead { addr: addr.to_string() };
            }
        } else if line.contains("st__") {
            if let (Some(reg), Some(value)) = (
                self.extract_register(line),
                self.extract_value(line)
            ) {
                return InstructionType::MemoryWrite { reg, value };
            }
        } else if line.contains("mov") || line.contains("ldr") ||
            line.contains("cbz") || line.contains("cbnz") {
            if let (Some(reg), Some(value)) = (
                self.extract_register(line),
                self.extract_value(line)
            ) {
                return InstructionType::RegTransfer { reg, value };
            }
        } else if line.contains("add") || line.contains("sub") {
            let regs = line.split(';')
                .filter(|p| p.starts_with("rr__"))
                .filter_map(|s| s.split('=').next())
                .filter_map(|s| s.strip_prefix("rr__"))
                .map(String::from)
                .collect();
            return InstructionType::Arithmetic { regs };
        }

        InstructionType::Other
    }

    // 处理内存读取 (ld)
    fn handle_memory_read(&mut self, current: &mut TracePath, line_num: usize, addr: &str, depth: usize) -> Option<()> {
        self.log(&format!("[内存读取] 地址: {}", addr));

        let pattern = format!("st__{}_[0-9]+", addr);
        if let Some(prev) = self.find_previous_instruction(line_num, &pattern, true) {
            self.log_line(prev.line_number);
            current.trace_type = TraceType::MemToReg;
            if let Some(source) = self.trace_backward_internal(prev.line_number, addr, depth + 1) {
                current.sources = vec![source];
            }
        } else {
            self.log("❌ 未找到内存写入");
            current.trace_type = TraceType::End;
        }
        Some(())
    }

    // 处理内存写入 (st)
    fn handle_memory_write(&mut self, current: &mut TracePath, line_num: usize, reg: &str, value: &str, depth: usize) -> Option<()> {
        self.log(&format!("[内存写入] 寄存器: {}, 值: {}", reg, value));

        // 搜索这个具体的值
        if let Some(prev) = self.find_previous_instruction(line_num, value, false) {
            self.log_line(prev.line_number);
            current.trace_type = TraceType::RegToMem;
            if let Some(source) = self.trace_backward_internal(prev.line_number, value, depth + 1) {
                current.sources = vec![source];
            }
        } else {
            self.log("❌ 未找到值的来源");
            current.trace_type = TraceType::End;
        }
        Some(())
    }

    // 处理寄存器传递
    fn handle_reg_transfer(&mut self, current: &mut TracePath, line_num: usize, value: &str, depth: usize) -> Option<()> {
        self.log(&format!("[寄存器传递] 值: {}", value));

        // 搜索这个具体的值
        if let Some(prev) = self.find_previous_instruction(line_num, value, false) {
            self.log_line(prev.line_number);
            current.trace_type = TraceType::RegToReg;
            if let Some(source) = self.trace_backward_internal(prev.line_number, value, depth + 1) {
                current.sources = vec![source];
            }
        } else {
            self.log("❌ 未找到值的来源");
            current.trace_type = TraceType::End;
        }
        Some(())
    }

    // 处理算术运算
    fn handle_arithmetic(&mut self, current: &mut TracePath, line_num: usize, regs: Vec<String>, depth: usize) -> Option<()> {
        self.log(&format!("[算术运算] 源寄存器: {:?}", regs));

        current.trace_type = TraceType::Arith;
        for reg in regs {
            // 对于算术运算，需要找到每个寄存器的值
            if let Some(value) = self.extract_register_value(line_num, &reg) {
                self.log(&format!("  ↳ 寄存器 {} 的值: {}", reg, value));

                if let Some(prev) = self.find_previous_instruction(line_num, &value, false) {
                    self.log(&format!("  ↳ 追踪分支: {}", reg));
                    if let Some(source) = self.trace_backward_internal(prev.line_number, &value, depth + 1) {
                        current.sources.push(source);
                    }
                }
            }
        }
        Some(())
    }

    // 辅助方法：提取寄存器名
    fn extract_register(&self, line: &str) -> Option<String> {
        line.split(';')
            .find(|p| p.starts_with("rr__"))
            .and_then(|s| s.split('=').next())
            .and_then(|s| s.strip_prefix("rr__"))
            .map(String::from)
    }

    // 辅助方法：提取值
    fn extract_value(&self, line: &str) -> Option<String> {
        line.split(';')
            .find(|p| p.starts_with("rr__"))
            .and_then(|s| s.split('=').nth(1))
            .map(|v| v.trim().to_string())
    }

    // 辅助方法：从指定行提取寄存器的值
    fn extract_register_value(&self, line_num: usize, reg: &str) -> Option<String> {
        let line = self.service.get_line_text(line_num)?;
        let pattern = format!("rr__{}=", reg);

        line.split(';')
            .find(|p| p.starts_with(&pattern))
            .and_then(|s| s.split('=').nth(1))
            .map(|v| v.trim().to_string())
    }

    // 查找前一条指令
    fn find_previous_instruction(&self, current_line: usize, pattern: &str, use_regex: bool) -> Option<SearchMatch> {
        let config = SearchConfig::new(pattern.to_string())
            .with_regex(use_regex)
            .with_max_results(1)
            .with_line_range(None, Some(current_line));

        self.service.find_prev(current_line, config)
    }

    // 日志辅助方法
    fn log(&self, message: &str) {
        if self.verbose {
            println!("{}", message);
        }
    }

    fn log_line(&self, line_num: usize) {
        if self.verbose {
            if let Some(text) = self.service.get_line_text(line_num) {
                println!("行{}: {}", line_num + 1, text);
            }
        }
    }
}

impl TracePath {
    pub fn print(&self) {
        self.print_internal(0);
    }

    fn print_internal(&self, indent: usize) {
        let indent_str = "  ".repeat(indent);
        println!("{}{} [行{}] {}",
                 indent_str,
                 self.trace_type,
                 self.line_num + 1,
                 self.instruction
        );
        for src in &self.sources {
            src.print_internal(indent + 1);
        }
    }
}


pub fn test_taint() -> anyhow::Result<()> {
    use large_text_core::file_reader::FileReader;

    let file_path = std::path::PathBuf::from("/Users/teng/RustroverProjects/large-text-viewer/logs/record.csv");
    let reader = FileReader::new(file_path, encoding_rs::UTF_8)?;
    let service = SearchService::new(reader);

    let mut engine = TaintEngine::new(service)
        .with_max_depth(15)
        .with_verbose(true);

    println!("\n=== 追踪内存地址: ld__6cf01586a0_4 ===\n");
    if let Some(trace) = engine.trace_backward(9028, "ld__6cf01586a0_4")? {
        // 如果需要打印结果
        // trace.print();
    }

    Ok(())
}