// taint_engine.rs
use crate::search_service::{SearchService, SearchConfig};
use crate::shadow_memory::ShadowMemory;
use crate::insn_analyzer::{InsnAnalyzer, InsnType};
use anyhow::Result;
use std::collections::HashSet;

const SEP: &str = ";";

pub struct TaintEngine {
    service: SearchService,
    shadow_mem: ShadowMemory,  // 新增: Shadow Memory
    max_depth: usize,
    visited: HashSet<usize>,
    debug: bool,  // 添加调试开关
    // 字节偏移追踪上下文：记录当前追踪的字节范围
    // key: (line_num, target_name), value: (byte_offset, byte_size)
    byte_context: std::collections::HashMap<(usize, String), (usize, usize)>,
}

#[derive(Debug, Clone)]
pub struct TracePath {
    pub line_num: usize,
    pub instruction: String,
    pub trace_type: TraceType,
    pub depth: usize,
    pub sources: Vec<TracePath>,
}

impl TracePath {
    /// 打印追踪路径树
    pub fn print(&self) {
        self.print_with_indent(0);
    }
    
    fn print_with_indent(&self, indent: usize) {
        let prefix = "  ".repeat(indent);
        let type_str = match &self.trace_type {
            TraceType::MemToReg(addr) => format!("📥 Mem->Reg ({})", addr),
            TraceType::RegToMem(reg) => format!("📤 Reg->Mem ({})", reg),
            TraceType::RegToReg(reg) => format!("🔄 Reg->Reg ({})", reg),
            TraceType::Arith(regs) => format!("🧮 Arith ({})", regs.join(", ")),
            TraceType::Constant => "🎯 Constant".to_string(),
            TraceType::Unknown => "❓ Unknown".to_string(),
        };
        
        println!("{}[{}] {} | {}", 
                 prefix, 
                 self.line_num + 1, 
                 type_str,
                 self.instruction.split(';').take(5).collect::<Vec<_>>().join(";"));
        
        for source in &self.sources {
            source.print_with_indent(indent + 1);
        }
    }
    
    /// 获取追踪深度
    pub fn max_depth(&self) -> usize {
        if self.sources.is_empty() {
            self.depth
        } else {
            self.sources.iter()
                .map(|s| s.max_depth())
                .max()
                .unwrap_or(self.depth)
        }
    }
    
    /// 统计追踪的指令数量
    pub fn count_instructions(&self) -> usize {
        1 + self.sources.iter().map(|s| s.count_instructions()).sum::<usize>()
    }
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
            shadow_mem: ShadowMemory::new(),  // 初始化 Shadow Memory
            max_depth: 10,
            visited: HashSet::new(),
            debug: false,  // 默认关闭调试
            byte_context: std::collections::HashMap::new(),
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

        // 使用 InsnAnalyzer 识别指令类型
        let insn_type = InsnAnalyzer::identify_insn_type(&line_text);

        match insn_type {
            InsnType::Load => {
                // 处理内存读取 (ld)
                if let Ok((dst_regs, addr, size)) = InsnAnalyzer::parse_load_insn(&line_text) {
                    // 检查是否有字节偏移上下文
                    let (adjusted_addr, adjusted_size) = if !dst_regs.is_empty() {
                        if let Some((byte_offset, byte_size)) = self.byte_context.get(&(line_num, dst_regs[0].clone())) {
                            // 调整搜索地址和大小
                            let new_addr = addr + *byte_offset as u64;
                            let new_size = *byte_size;
                            println!("  [字节追踪] 调整搜索: 0x{:x}[{}] -> 0x{:x}[{}]", 
                                     addr, size, new_addr, new_size);
                            (new_addr, new_size)
                        } else {
                            (addr, size)
                        }
                    } else {
                        (addr, size)
                    };
                    
                    path.trace_type = TraceType::MemToReg(format!("0x{:x}", adjusted_addr));
                    path.sources = self.trace_mem_read(line_num, adjusted_addr, adjusted_size, &dst_regs, depth)
                        .into_iter().collect();
                }
            }
            InsnType::Store => {
                // 处理内存写入 (st)
                if let Ok((src_regs, addr, size)) = InsnAnalyzer::parse_store_insn(&line_text) {
                    path.trace_type = TraceType::RegToMem(src_regs.join(","));
                    path.sources = self.trace_mem_write(line_num, &src_regs, addr, size, depth)
                        .into_iter().collect();
                }
            }
            InsnType::Move => {
                // 处理寄存器传递 (mov)
                if let Some((src_reg, value)) = self.extract_reg_value(&line_text) {
                    path.trace_type = TraceType::RegToReg(src_reg.clone());
                    path.sources = self.trace_reg_transfer(line_num, &src_reg, &value, depth)
                        .into_iter().collect();
                }
            }
            InsnType::Arith | InsnType::Logic => {
                // 处理算术/逻辑运算
                println!("[AlgOp]");
                let src_regs = self.extract_reg_pairs(&line_text);
                path.trace_type = TraceType::Arith(src_regs.iter().map(|s| s.to_string()).collect());
                path.sources = self.trace_arith_operation(line_num, src_regs, depth);
            }
            InsnType::Branch => {
                // 分支指令，追踪条件寄存器
                if let Some((src_reg, value)) = self.extract_reg_value(&line_text) {
                    path.trace_type = TraceType::RegToReg(src_reg.clone());
                    path.sources = self.trace_reg_transfer(line_num, &src_reg, &value, depth)
                        .into_iter().collect();
                }
            }
            InsnType::Unknown => {
                // 终点/常量
                println!("终点/常量");
                path.trace_type = TraceType::Constant;
            }
        }

        Some(path)
    }

    // 辅助方法：提取寄存器和值（保留用于兼容性）
    fn extract_reg_value(&self, line_text: &str) -> Option<(String, String)> {
        let values = InsnAnalyzer::extract_reg_values(line_text, "rr__");
        values.first().cloned()
    }

    // 辅助方法：提取所有源寄存器对（保留用于兼容性）
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

    // 追踪内存读取（使用启发式搜索策略）
    fn trace_mem_read(&mut self, line_num: usize, addr: u64, size: usize, dst_regs: &[String], depth: usize) -> Option<TracePath> {
        // 生成按优先级排序的搜索 patterns
        let search_patterns = InsnAnalyzer::gen_mem_read_patterns(addr, size);
        
        println!("[mem2mem]: 启发式搜索 0x{:x} ({} 字节)", addr, size);
        
        // 按优先级依次尝试每个 pattern
        for (priority, pattern) in search_patterns.iter().enumerate() {
            if self.debug {
                println!("  [优先级 {}] {}: {}", priority + 1, pattern.description, pattern.pattern);
            }
            
            let config = SearchConfig::new(pattern.pattern.clone()).with_regex(pattern.is_regex);
            
            // 查找匹配的指令
            let mut current_line = line_num;
            loop {
                if let Some(prev) = self.service.find_prev(current_line, config.clone()) {
                    let prev_line_text = self.service.get_line_text(prev.line_number)?;
                    
                    // 解析写入指令
                    if let Ok((src_regs, write_addr, write_size)) = InsnAnalyzer::parse_store_insn(&prev_line_text) {
                        // 检查内存重叠
                        if let Some((write_offset, overlap_size)) = InsnAnalyzer::check_memory_overlap(
                            addr, size, write_addr, write_size
                        ) {
                            println!("  ✓ 找到匹配 [行 {}]: write[0x{:x}+{}:{}] -> read[0x{:x}:0x{:x}]", 
                                     prev.line_number + 1,
                                     write_addr,
                                     write_offset,
                                     write_offset + overlap_size,
                                     addr,
                                     addr + size as u64);
                            
                            if self.debug {
                                println!("    {}", prev_line_text.split(';').take(5).collect::<Vec<_>>().join(";"));
                            }

                            // 使用 shadow memory 传播污点（字节级）
                            for dst_reg in dst_regs {
                                // 从写入的偏移位置传播到目标寄存器
                                self.shadow_mem.propagate_mem_to_reg(addr, size, dst_reg, 0);
                            }
                            
                            // 关键：追踪源寄存器的特定字节范围
                            // 如果 write_offset != 0，说明我们只需要源寄存器的部分字节
                            if !src_regs.is_empty() {
                                let src_reg = &src_regs[0];
                                
                                // 记录字节偏移上下文
                                if write_offset > 0 || overlap_size < write_size {
                                    println!("  → 追踪 {} 的字节 [{}:{}]", src_reg, write_offset, write_offset + overlap_size);
                                    // 保存字节偏移信息，供下一层使用
                                    self.byte_context.insert(
                                        (prev.line_number, src_reg.clone()),
                                        (write_offset, overlap_size)
                                    );
                                }
                                
                                // 继续追踪源寄存器
                                return self._trace_backward(prev.line_number, src_reg, depth + 1);
                            }

                            return self._trace_backward(prev.line_number, &format!("0x{:x}", addr), depth + 1);
                        } else {
                            // 地址不匹配，继续向前搜索
                            if self.debug {
                                println!("    [跳过] 地址不匹配: 0x{:x} vs 0x{:x}", write_addr, addr);
                            }
                            current_line = prev.line_number;
                            continue;
                        }
                    }
                    
                    // 解析失败，继续搜索
                    current_line = prev.line_number;
                } else {
                    // 当前 pattern 没有找到结果，尝试下一个优先级
                    if self.debug {
                        println!("  ✗ 未找到匹配");
                    }
                    break;
                }
            }
        }
        
        println!("❌ 所有搜索策略均未找到来源");
        None
    }

    // 追踪内存写入（使用 InsnAnalyzer 和 ShadowMemory）
    fn trace_mem_write(&mut self, line_num: usize, src_regs: &[String], addr: u64, size: usize, depth: usize) -> Option<TracePath> {
        if src_regs.is_empty() {
            return None;
        }

        // 取第一个源寄存器进行追踪
        let src_reg = &src_regs[0];
        
        // 从指令中提取寄存器值
        let line_text = self.service.get_line_text(line_num)?;
        let reg_values = InsnAnalyzer::extract_reg_values(&line_text, "rr__");
        
        if let Some((_, value)) = reg_values.iter().find(|(r, _)| r == src_reg) {
            // 使用 shadow memory 传播污点
            self.shadow_mem.propagate_reg_to_mem(src_reg, 0, addr, size);
            
            // 生成搜索 pattern
            let search_pattern = InsnAnalyzer::gen_reg_write_pattern(src_reg, value);
            println!("[regW] {}", search_pattern.pattern);
            
            let config = SearchConfig::new(search_pattern.pattern).with_regex(search_pattern.is_regex);
            
            return self.find_and_trace(line_num, &config, src_reg, depth);
        }
        
        None
    }

    // 追踪寄存器传递,从rr__到rw__（使用 InsnAnalyzer）
    fn trace_reg_transfer(&mut self, line_num: usize, reg: &str, _value: &str, depth: usize) -> Option<TracePath> {
        // 检查是否是零寄存器（常量）
        if InsnAnalyzer::is_zero_register(reg) {
            println!("终点: 零寄存器 {}", reg);
            return None;
        }
        
        let search_pattern = InsnAnalyzer::gen_reg_read_pattern(reg, _value);
        println!("[reg2reg]: {}", search_pattern.pattern);
        
        let config = SearchConfig::new(search_pattern.pattern).with_regex(search_pattern.is_regex);

        self.find_and_trace(line_num, &config, reg, depth)
    }

    // 追踪算术运算（使用 InsnAnalyzer）
    fn trace_arith_operation(&mut self, line_num: usize, regs: Vec<String>, depth: usize) -> Vec<TracePath> {
        let mut sources = Vec::new();
        
        // 解析寄存器值对
        let reg_values: Vec<(String, String)> = regs.iter()
            .filter_map(|pair| {
                pair.split_once('=')
                    .map(|(r, v)| (r.to_string(), v.to_string()))
            })
            .collect();
        
        // 生成搜索 patterns
        let patterns = InsnAnalyzer::gen_arith_patterns(&reg_values);
        
        for (pattern, (reg, val)) in patterns.iter().zip(reg_values.iter()) {
            // 跳过零寄存器和立即数
            if InsnAnalyzer::is_zero_register(reg) {
                println!("[arith] 跳过零寄存器: {}", reg);
                continue;
            }
            
            if InsnAnalyzer::is_constant_value(val) {
                println!("[arith] 跳过常量值: {}", val);
                continue;
            }
            
            println!("[arith] {}", pattern.pattern);
            let config = SearchConfig::new(pattern.pattern.clone()).with_regex(pattern.is_regex);

            if let Some(prev) = self.service.find_prev(line_num, config) {
                println!("\t{}: {}", prev.line_number + 1,
                         self.service.get_line_text(prev.line_number).unwrap_or_default());

                if let Some(source) = self._trace_backward(prev.line_number, reg, depth + 1) {
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
    // if let Some(trace) = engine.trace_backward(9028, "ld__6cf01586a0_4")? {
    if let Some(trace) = engine.trace_backward(9218-1, "ld__6cf01586a8_8")? {
        // trace.print();
    }

    Ok(())
}

pub fn test_taint_overlap() -> anyhow::Result<()> {
    use large_text_core::file_reader::FileReader;

    let file_path = std::path::PathBuf::from("/Users/teng/RustroverProjects/large-text-viewer/logs/record_01.csv");
    let reader = FileReader::new(file_path, encoding_rs::UTF_8)?;
    let service = SearchService::new(reader);

    let mut engine = TaintEngine::new(service)
        .with_max_depth(15)
        .with_debug(true);  // 启用调试模式

    println!("\n=== 追踪内存地址: ld__6cf01586a8_8 (测试内存重叠) ===\n");
    // ldr x21, [sp, #0xc08] 读取 0x6cf01586a8 (8字节)
    // 应该找到 str q0, [x19] 写入 0x6cf01586a0 (16字节)
    if let Some(trace) = engine.trace_backward(9217, "ld__6cf01586a8_8")? {
        println!("\n=== 追踪结果 ===\n");
        trace.print();
        println!("\n统计信息:");
        println!("  - 最大深度: {}", trace.max_depth());
        println!("  - 指令数量: {}", trace.count_instructions());
    }

    Ok(())
}

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
