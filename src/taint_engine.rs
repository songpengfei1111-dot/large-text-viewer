// taint_engine.rs
use crate::search_service::{SearchService, SearchConfig};
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

#[derive(Debug, Clone)]
pub enum TraceType {
    MemToReg(String),      // 内存到寄存器，携带内存地址
    RegToMem(String),      // 寄存器到内存，携带寄存器名
    RegToReg(String),      // 寄存器传递，携带源寄存器名
    Arith(Vec<String>),    // 算术运算，携带源寄存器列表
    Constant,              // 常量/终点
    Unknown,
}

impl TraceType {
    fn as_str(&self) -> &'static str {
        match self {
            TraceType::MemToReg(_) => "MEM→REG",
            TraceType::RegToMem(_) => "REG→MEM",
            TraceType::RegToReg(_) => "REG→REG",
            TraceType::Arith(_) => "ARITH",
            TraceType::Constant => "CONST",
            TraceType::Unknown => "UNKNOWN",
        }
    }
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

    pub fn trace_backward(&mut self, start_line: usize, target: &str) -> Result<Option<TracePath>> {
        self.visited.clear();
        if self.verbose {
            println!("\n=== 开始反向追踪: {} 从行{} ===\n", target, start_line + 1);
        }
        Ok(self._trace_backward(start_line, target, 0))
    }

    fn _trace_backward(&mut self, line_num: usize, target: &str, depth: usize) -> Option<TracePath> {
        if depth >= self.max_depth { return None;}

        self.visited.insert(line_num);

        let line_text = self.service.get_line_text(line_num)?;

        // 使用链表，这样后面还可以树拓展
        let mut path = TracePath {
            line_num,
            instruction: line_text.clone(),
            trace_type: TraceType::Unknown,
            depth,
            sources: vec![],
        };

        // 处理内存读取 (ld)
        if line_text.contains("ld__") {
            if let Some(ld_addr) = self.extract_ld_addr(&line_text) {
                path.trace_type = TraceType::MemToReg(target.to_string());
                path.sources = self.trace_mem_read(line_num, &ld_addr, depth)
                    .into_iter().collect();
            }
        }
        // 处理内存写入 (st)
        else if line_text.contains("st__") {
            if let Some((reg, value)) = self.extract_reg_value(&line_text) {
                path.trace_type = TraceType::RegToMem(reg.clone());
                path.sources = self.trace_mem_write(line_num, &reg, &value, depth)
                    .into_iter().collect();
            }
        }
        // 处理寄存器传递 (mov/ldr/cbz/cbnz)
        else if self.is_reg_transfer_insn(&line_text) {
            if let Some((src_reg, value)) = self.extract_reg_value(&line_text) {

                path.trace_type = TraceType::RegToReg(src_reg.clone());
                path.sources = self.trace_reg_transfer(line_num, &src_reg, &value, depth)
                    .into_iter().collect();
            }
        }
        // 处理算术运算
        else if self.is_arith_insn(&line_text) {
            println!("[AlgOp]");
            let src_regs = self.extract_src_regs(&line_text);
            println!("\tsrcReg: {:?}", src_regs);

            path.trace_type = TraceType::Arith(src_regs.clone());
            path.sources = self.trace_arith_operation(line_num, src_regs, depth);
        }
        // 终点/常量
        else {
            if self.verbose {
                println!("终点/常量");
            }
            path.trace_type = TraceType::Constant;
        }

        Some(path)
    }

    // 辅助方法：提取ld指令的内存地址
    fn extract_ld_addr(&self, line_text: &str) -> Option<String> {
        //TODO 这里可以简化，毕竟是结构化数据
        line_text.split(';')
            .find(|p| p.contains("ld__"))
            .map(|addr| {
                addr.rsplit('_').nth(1)
                    .unwrap_or(addr)
                    .to_string()
            })
    }

    // 辅助方法：提取寄存器和值
    fn extract_reg_value(&self, line_text: &str) -> Option<(String, String)> {
        let reg = line_text.split(';')
            .find(|p| p.starts_with("rr__"))
            .and_then(|s| s.split('=').next())
            .and_then(|s| s.strip_prefix("rr__"))?;

        let value = line_text.split(';')
            .find(|p| p.starts_with(&format!("rr__{}=", reg)))
            .and_then(|s| s.split('=').nth(1))?;

        Some((reg.to_string(), value.trim().to_string()))
    }

    // 辅助方法：提取所有源寄存器
    fn extract_src_regs(&self, line_text: &str) -> Vec<String> {
        line_text.split(';')
            .filter(|p| p.starts_with("rr__"))
            .filter_map(|s| s.split('=').next())
            .filter_map(|s| s.strip_prefix("rr__"))
            .map(String::from)
            .collect()
    }

    // 辅助方法：判断是否是寄存器传递指令
    fn is_reg_transfer_insn(&self, line_text: &str) -> bool {
        // TODO改成 in list
        line_text.contains("mov") ||
            line_text.contains("ldr") ||
            line_text.contains("cbz") ||
            line_text.contains("cbnz")
    }

    // 辅助方法：判断是否是算术指令
    fn is_arith_insn(&self, line_text: &str) -> bool {
        line_text.contains("add") || line_text.contains("sub")
    }

    // 追踪内存读取
    fn trace_mem_read(&mut self, line_num: usize, addr: &str, depth: usize) -> Option<TracePath> {
        let pattern = format!("st__{}_", addr);
        println!("[mem2mem]: {}", pattern);
        let config = SearchConfig::new(pattern)
            .with_regex(true);

        self.find_and_trace(line_num, &config, addr, depth)
    }

    // 追踪内存写入
    fn trace_mem_write(&mut self, line_num: usize, reg: &str, value: &str, depth: usize) -> Option<TracePath> {
        let pattern = format!("rw_.*{}={}", &reg[1..],value);
        println!("[regW] {}", pattern);

        let config = SearchConfig::new(pattern)
            .with_regex(true);

        self.find_and_trace(line_num, &config, reg, depth)
    }

    // 追踪寄存器传递
    fn trace_reg_transfer(&mut self, line_num: usize, reg: &str, _value: &str, depth: usize) -> Option<TracePath> {
        let pattern = format!("r[wr]__{}=", reg);
        println!("[reg2reg]: {}", pattern);
        let config = SearchConfig::new(pattern)
            .with_regex(true)
            .with_max_results(1)
            .with_line_range(None, Some(line_num));

        self.find_and_trace(line_num, &config, reg, depth)
    }

    // 追踪算术运算
    fn trace_arith_operation(&mut self, line_num: usize, regs: Vec<String>, depth: usize) -> Vec<TracePath> {
        let mut sources = Vec::new();
        // 先判断是否相同，再逐个跟踪
        for reg in regs {
            let config = SearchConfig::new(format!("r[wr]__{}=", reg))
                .with_regex(true)
                .with_max_results(1)
                .with_line_range(None, Some(line_num));

            if let Some(prev) = self.service.find_prev(line_num, config) {
                if self.verbose {
                    println!("  ↳ 追踪分支: {}", reg);
                }
                if let Some(source) = self._trace_backward(prev.line_number, &reg, depth + 1) {
                    sources.push(source);
                }
            }
        }
        sources
    }

    // 通用查找并追踪（通用执行函数）
    fn find_and_trace(&mut self, line_num: usize, config: &SearchConfig, target: &str, depth: usize) -> Option<TracePath> {
        self.service.find_prev(line_num, config.clone())
            .and_then(|prev| {
                println!("{}: {}", prev.line_number + 1,
                         self.service.get_line_text(prev.line_number).unwrap_or_default());
                self._trace_backward(prev.line_number, target, depth + 1)
            })
            .or_else(|| {
                println!("❌ 未找到来源");
                None
            })
    }
}

pub fn test_taint() -> anyhow::Result<()> {
    use large_text_core::file_reader::FileReader;

    let file_path = std::path::PathBuf::from("/Users/teng/RustroverProjects/large-text-viewer/logs/record.csv");
    let reader = FileReader::new(file_path, encoding_rs::UTF_8)?;
    let service = SearchService::new(reader);

    let mut engine = TaintEngine::new(service).with_max_depth(15);

    println!("\n=== 追踪内存地址: ld__6cf01586a0_4 ===\n");
    if let Some(trace) = engine.trace_backward(9028, "ld__6cf01586a0_4")? {
        // trace.print();
    }

    Ok(())
}

//shdow_mem