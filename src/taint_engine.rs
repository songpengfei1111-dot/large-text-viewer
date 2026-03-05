// taint_engine.rs
use crate::search_service::{SearchService, SearchConfig};
use anyhow::Result;
use std::collections::HashSet;

const SEP: &str = ";";

pub struct TaintEngine {
    service: SearchService,
    max_depth: usize,
    visited: HashSet<usize>,
    debug: bool,  // 添加调试开关
}

#[derive(Debug, Clone)]
pub struct TracePath {
    pub line_num: usize,
    pub instruction: String,
    pub trace_type: TraceType,
    pub depth: usize,
    pub sources: Vec<TracePath>,
    // pub search_pattern: Option<String>,
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

impl TaintEngine {
    pub fn new(service: SearchService) -> Self {
        Self {
            service,
            max_depth: 10,
            visited: HashSet::new(),
            debug: false,  // 默认关闭调试
        }
    }

    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = depth;
        self
    }
    
    pub fn with_debug(mut self, debug: bool) -> Self {
        self.debug = debug;
        self
    }

    pub fn trace_backward(&mut self, start_line: usize, target: &str) -> Result<Option<TracePath>> {
        self.visited.clear();
        println!("\n=== 开始反向追踪: {} 从行{} ===\n", target, start_line + 1);

        Ok(self._trace_backward(start_line, target, 0))
    }

    fn _trace_backward(&mut self, line_num: usize, target: &str, depth: usize) -> Option<TracePath> {
        // 优化：提前检查深度和访问状态
        if depth >= self.max_depth { 
            return None;
        }
        
        // 优化：避免重复追踪同一行
        if self.visited.contains(&line_num) {
            return None;
        }
        
        self.visited.insert(line_num);

        let line_text = self.service.get_line_text(line_num)?;
        if (depth == 0){ println!("[target line]: {}",line_text);}

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
            if let Some((src_reg, value)) = self.extract_reg_value(&line_text) {
                path.trace_type = TraceType::RegToMem(src_reg.clone());
                path.sources = self.trace_mem_write(line_num, &src_reg, &value, depth)
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
            let src_regs = self.extract_reg_pairs(&line_text);
            path.trace_type = TraceType::Arith(src_regs.clone());
            path.sources = self.trace_arith_operation(line_num, src_regs, depth);
        }
        // 终点/常量
        else {
            println!("终点/常量");
            path.trace_type = TraceType::Constant;
        }

        Some(path)
    }

    // 辅助方法：提取ld指令的内存地址（优化：减少字符串分配）
    fn extract_ld_addr(&self, line_text: &str) -> Option<String> {
        line_text.split(SEP)
            .find(|p| p.contains("ld__"))
            .and_then(|addr| {
                addr.rsplit('_').nth(1)
                    .map(|s| s.to_string())
            })
    }

    // 辅助方法：提取寄存器和值
    fn extract_reg_value(&self, line_text: &str) -> Option<(String, String)> {
        // TODO 对于多个寄存器的处理
        line_text.split(SEP)
            .find(|p| p.starts_with("rr__"))
            .and_then(|part| part.strip_prefix("rr__"))
            .and_then(|s| s.split('_').next())
            .and_then(|first_pair| first_pair.split_once('='))
            .map(|(reg, val)| (reg.to_string(), val.to_string()))
    }

    fn extract_reg_values(line_text: &str) -> Vec<(String, String)> {
        line_text.split(SEP)
            .find(|p| p.starts_with("rr__"))
            .and_then(|part| part.strip_prefix("rr__"))
            .map(|s| {
                s.split('_')
                    .filter(|pair| pair.contains('='))
                    .filter_map(|pair| pair.split_once('='))
                    .map(|(reg, val)| (reg.to_string(), val.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }

    // 辅助方法：提取所有源寄存器
    fn extract_src_regs(&self, line_text: &str) -> Vec<String> {
        line_text.split(SEP)
            .filter(|p| p.starts_with("rr__"))
            .filter_map(|s| s.split('=').next())
            .filter_map(|s| s.strip_prefix("rr__"))
            .map(String::from)
            .collect()
    }

    //["w22=0x1", "w22=0x1"]
    fn extract_reg_pairs(&self, line_text: &str) -> Vec<String> {
        line_text.split(SEP)
            .find(|p| p.starts_with("rr__"))
            .and_then(|part| part.strip_prefix("rr__"))
            .map(|s| {
                s.split('_')
                    .filter(|pair| pair.contains('='))
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default()
    }


    // 辅助方法：判断是否是寄存器传递指令（优化：减少重复扫描）
    fn is_reg_transfer_insn(&self, line_text: &str) -> bool {
        // 优化：只扫描一次字符串
        line_text.contains("mov") || line_text.contains("ldr") || 
        line_text.contains("cbz") || line_text.contains("cbnz")
    }

    // 辅助方法：判断是否是算术指令（优化：减少重复扫描）
    fn is_arith_insn(&self, line_text: &str) -> bool {
        line_text.contains("add") || line_text.contains("sub")
    }

    // 追踪内存读取
    fn trace_mem_read(&mut self, line_num: usize, addr: &str, depth: usize) -> Option<TracePath> {
        // 这里要使用shawo mem分析
        let pattern = format!("st__{}_", addr);
        println!("[mem2mem]: {}", pattern);
        let config = SearchConfig::new(pattern).with_regex(false);

        self.find_and_trace(line_num, &config, addr, depth)
    }

    // 追踪内存写入
    fn trace_mem_write(&mut self, line_num: usize, reg: &str, value: &str, depth: usize) -> Option<TracePath> {
        let pattern = format!("rw_.*{}={}", &reg[1..], value);
        println!("[regW] {}", pattern);

        let config = SearchConfig::new(pattern).with_regex(true);

        self.find_and_trace(line_num, &config, reg, depth)
    }

    // 追踪寄存器传递,从rr__到rw__
    fn trace_reg_transfer(&mut self, line_num: usize, reg: &str, _value: &str, depth: usize) -> Option<TracePath> {
        let pattern = format!("rr__{}={}", reg,_value);
        println!("[reg2reg]: {}", pattern);
        let config = SearchConfig::new(pattern).with_regex(true);

        self.find_and_trace(line_num, &config, reg, depth)
    }

    // 追踪算术运算
    fn trace_arith_operation(&mut self, line_num: usize, regs: Vec<String>, depth: usize) -> Vec<TracePath> {
        let mut sources = Vec::new();
        // 先判断是否相同，再逐个跟踪
        for reg in regs {
            let pattern = format!("rw_.*{}", &reg[1..]); //考虑特殊寄存器
            println!("[arith] {}", pattern);
            let config = SearchConfig::new(pattern).with_regex(true);


            if let Some(prev) = self.service.find_prev(line_num, config) {
                println!("\t{}: {}", prev.line_number + 1,
                         self.service.get_line_text(prev.line_number).unwrap_or_default());

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
                println!("\t{}: {}", prev.line_number + 1, self.service.get_line_text(prev.line_number).unwrap_or_default());
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

    let file_path = std::path::PathBuf::from("/Users/teng/RustroverProjects/large-text-viewer/logs/record_01.csv");
    let reader = FileReader::new(file_path, encoding_rs::UTF_8)?;
    let service = SearchService::new(reader);

    let mut engine = TaintEngine::new(service).with_max_depth(15);

    println!("\n=== 追踪内存地址: ld__6cf01586a0_4 ===\n");
    if let Some(trace) = engine.trace_backward(9028, "ld__6cf01586a0_4")? {
        // trace.print();
    }

    Ok(())
}

// 需要完善的 shdow_mem 和 reg解析

pub fn test_taint_1() -> anyhow::Result<()> {
    use large_text_core::file_reader::FileReader;

    let file_path = std::path::PathBuf::from("/Users/teng/RustroverProjects/large-text-viewer/logs/record_01.csv");
    let reader = FileReader::new(file_path, encoding_rs::UTF_8)?;
    let service = SearchService::new(reader);

    let mut engine = TaintEngine::new(service).with_max_depth(15);

    println!("\n=== 追踪内存地址: ===\n");
    // if let Some(trace) = engine.trace_backward(11923, "st__6cf0157918_8")? { //err
    if let Some(trace) = engine.trace_backward(11922-1, "st__6cf0157918_8")? {
        // trace.print();
    }

    Ok(())
}
