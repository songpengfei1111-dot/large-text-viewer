// taint_engine.rs
use crate::search_service::{SearchService, SearchConfig};
use crate::insn_analyzer::{InsnType, ParsedInsn};
use crate::trace_path_tree::TracePathTree;
use anyhow::Result;
use std::collections::HashSet;

const SEP: &str = ";";

// 寄存器字段前缀常量
const PREFIX_REG_READ: &str = "rr__";
const PREFIX_REG_WRITE: &str = "rw__";

pub struct TaintEngine {
    service: SearchService,
    max_depth: usize,
    visited: HashSet<usize>,
    debug: bool,  // 添加调试开关
    // 字节偏移追踪上下文：记录当前追踪目标的字节范围
    // key: target_name (寄存器名), value: (byte_offset, byte_size)
    current_byte_range: Option<(String, usize, usize)>,
}

#[derive(Debug, Clone)]
pub struct TracePath {
    pub line_num: usize,
    pub instruction: String,
    pub trace_type: TraceType,
    pub depth: usize,
    pub sources: Vec<TracePath>, // 溯源
    pub parsed_insn: Option<crate::insn_analyzer::ParsedInsn>,
}

impl TracePath {
    /// 打印追踪路径树
    /// 构造一个树节点
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

    /// 将 TracePath 渲染为二叉树
    pub fn render_as_tree(&self) {
        use crate::trace_path_tree::TracePathTree;
        println!("\n=== 渲染追踪路径为二叉树 ===\n");
        let tree = TracePathTree::from_trace_path(self);
        tree.render();
        println!("\n=== 二叉树渲染完成 ===\n");
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
            max_depth: 10,
            visited: HashSet::new(),
            debug: false,  // 默认关闭调试
            current_byte_range: None,
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

    // 统一的调试输出
    fn debug_log(&self, msg: &str) {
        if self.debug {
            println!("{}", msg);
        }
    }
    


    pub fn trace_backward(&mut self, start_line: usize, target: &str) -> Result<Option<TracePath>> {
        self.visited.clear();
        println!("\n=== 开始反向追踪: {} 从行{} ===\n", target, start_line + 1);

        Ok(self._trace_backward(start_line, target, 0))
    }

    //递归调用这个函数
    fn _trace_backward(&mut self, line_num: usize, target: &str, depth: usize) -> Option<TracePath> {
        if depth >= self.max_depth { return None; }

        self.visited.insert(line_num);

        let line_text = self.service.get_line_text(line_num)?;
        if depth == 0 { println!("[target line]: {}", line_text); }

        let parsed = ParsedInsn::parse(&line_text);

        let mut path = TracePath {
            line_num,
            instruction: line_text.clone(),
            trace_type: TraceType::Unknown,
            depth,
            sources: vec![],
            parsed_insn: Some(parsed.clone()),
        };

        // 根据类型分发
        match parsed.insn_type {
            InsnType::Load => {
                if let Ok((dst_regs, addr, size)) = parsed.get_load_info() {
                    self.debug_log(&format!("  [Load] 目标寄存器: {:?}, target={}, byte_range={:?}", dst_regs, target, self.current_byte_range));
                    
                    let (adjusted_addr, adjusted_size) = self.calculate_adjusted_address(&dst_regs, addr, size, target);
                    
                    path.trace_type = TraceType::MemToReg(format!("0x{:x}", adjusted_addr));
                    path.sources.extend(
                        self.trace_mem_read(line_num, adjusted_addr, adjusted_size, &dst_regs, depth)
                    );
                }
            }
            InsnType::Store => {
                if let Ok((src_regs, addr, size)) = parsed.get_store_info() {
                    path.trace_type = TraceType::RegToMem(src_regs.join(","));
                    path.sources.extend(self.trace_mem_write(line_num, &src_regs, addr, size, depth));
                }
            }
            InsnType::Arith => {
                // 所有非 load/store 指令：算术、逻辑、分支、寄存器传递等
                if parsed.read_regs.is_empty() {
                    // 没有读取寄存器，说明是常量或终点
                    println!("终点/常量");
                    path.trace_type = TraceType::Constant;
                } else if parsed.read_regs.len() == 1 {
                    // 单个源寄存器：寄存器传递或分支
                    println!("[Reg Transfer/Branch]");
                    let (src_reg, value) = &parsed.read_regs[0];
                    path.trace_type = TraceType::RegToReg(src_reg.clone());
                    path.sources.extend(
                        self.trace_reg_transfer(line_num, src_reg, value, depth)
                    );
                } else {
                    // 多个源寄存器：算术或逻辑运算，产生追踪分支
                    println!("[Arith/Logic] 🌳 追踪分支: {} 个源寄存器", parsed.read_regs.len());
                    let src_regs: Vec<String> = parsed.read_regs.iter()
                        .map(|(reg, val)| format!("{}={}", reg, val))
                        .collect();
                    
                    path.trace_type = TraceType::Arith(parsed.read_regs.iter().map(|(r, _)| r.clone()).collect());
                    path.sources = self.trace_arith_operation(line_num, src_regs, depth);
                    
                    if !path.sources.is_empty() {
                        println!("  ✓ 成功追踪 {} 个分支", path.sources.len());
                    }
                }
            }
        }

        Some(path)
    }
    /// 计算调整后的内存地址和大小（处理多寄存器和字节偏移,内存对齐）
    fn calculate_adjusted_address(
        &self,
        dst_regs: &[String],
        addr: u64,
        size: usize,
        target: &str,
    ) -> (u64, usize) {
        if dst_regs.is_empty() {
            return (addr, size);
        }

        // 情况1: 有字节偏移上下文
        if let Some((target_reg, byte_offset, byte_size)) = &self.current_byte_range {
            if let Some(reg_index) = dst_regs.iter().position(|r| r == target_reg) {
                let reg_size = ParsedInsn::get_reg_size(target_reg);
                let mem_offset = reg_index * reg_size;
                let new_addr = addr + mem_offset as u64 + *byte_offset as u64;
                println!("  [字节追踪] 寄存器 {} 在位置 {}, 调整搜索: 0x{:x}[{}] -> 0x{:x}[{}]",
                         target_reg, reg_index, addr, size, new_addr, *byte_size);
                return (new_addr, *byte_size);
            }
        }

        // 情况2: 多寄存器指令
        if dst_regs.len() > 1 {
            if let Some(reg_index) = dst_regs.iter().position(|r| r == target) {
                let reg_size = ParsedInsn::get_reg_size(target);
                let mem_offset = reg_index * reg_size;
                let new_addr = addr + mem_offset as u64;
                println!("  [多寄存器] 寄存器 {} 在位置 {}, 调整搜索: 0x{:x}[{}] -> 0x{:x}[{}]",
                         target, reg_index, addr, size, new_addr, reg_size);
                return (new_addr, reg_size);
            }
        }

        (addr, size)
    }

    /// 验证 store 指令的值是否匹配（仅在寄存器类型相同时校验）
    fn validate_store_value(
        &mut self,
        load_line_num: usize,
        store_line_num: usize,
        dst_regs: &[String],
        src_regs: &[String],
        write_offset: usize,
    ) -> bool {
        let load_parsed = match self.service.get_line_text(load_line_num) {
            Some(text) => ParsedInsn::parse(&text),
            None => return true,
        };
        
        let store_parsed = match self.service.get_line_text(store_line_num) {
            Some(text) => ParsedInsn::parse(&text),
            None => return true,
        };
        
        // 确定目标寄存器
        let target_reg = self.current_byte_range.as_ref()
            .map(|(reg, _, _)| reg.clone())
            .or_else(|| dst_regs.first().cloned())
            .unwrap_or_default();
        
        if target_reg.is_empty() {
            return true;
        }
        
        // 提取期望值
        let expected_value = match load_parsed.write_regs.iter().find(|(reg, _)| reg == &target_reg) {
            Some((_, val)) => val,
            None => return true,
        };
        
        // 找到对应偏移位置的源寄存器
        let src_reg_index = (write_offset / 8).min(src_regs.len().saturating_sub(1));
        let src_reg = match src_regs.get(src_reg_index) {
            Some(reg) => reg,
            None => return true,
        };
        
        // 检查寄存器类型是否相同
        if target_reg.chars().next() != src_reg.chars().next() {
            println!("  ℹ 寄存器类型不同 ({:?} vs {:?})，跳过值校验", 
                     target_reg.chars().next(), src_reg.chars().next());
            return true;
        }
        
        // 提取实际值并校验
        match store_parsed.read_regs.iter().find(|(r, _)| r == src_reg) {
            Some((_, actual_val)) if actual_val != expected_value => {
                println!("  ⚠ 值不匹配: 期望 {}, 实际 {} (寄存器 {})", 
                         expected_value, actual_val, src_reg);
                println!("  → 继续搜索其他候选...");
                false
            }
            Some(_) => {
                println!("  ✓ 值校验通过: {} = {}", src_reg, expected_value);
                true
            }
            None => true,
        }
    }

    /// 追踪源寄存器（处理字节偏移上下文）
    fn trace_source_register(
        &mut self,
        prev_line_num: usize,
        src_regs: &[String],
        write_offset: usize,
        overlap_size: usize,
        write_size: usize,
        depth: usize,
    ) -> Option<TracePath> {
        if src_regs.is_empty() {
            return None;
        }
        
        // 计算应该追踪哪个源寄存器
        let src_reg_index = (write_offset / 8).min(src_regs.len().saturating_sub(1));
        let src_reg = &src_regs[src_reg_index];
        let reg_size = ParsedInsn::get_reg_size(src_reg);
        
        // 计算在该源寄存器内的偏移
        let reg_internal_offset = write_offset.saturating_sub(src_reg_index * reg_size);
        
        // 保存字节偏移上下文，供下一层使用
        let old_byte_range = self.current_byte_range.clone();
        if write_offset > 0 || overlap_size < write_size {
            println!("  → 追踪 {} 的字节 [{}:{}]", 
                     src_reg, reg_internal_offset, reg_internal_offset + overlap_size);
            self.current_byte_range = Some((src_reg.clone(), reg_internal_offset, overlap_size));
        }
        
        // 继续追踪源寄存器
        let result = self._trace_backward(prev_line_num, src_reg, depth + 1);
        
        // 恢复之前的上下文
        self.current_byte_range = old_byte_range;
        
        result
    }

    // 追踪内存读取（使用启发式搜索策略）
    fn trace_mem_read(&mut self, line_num: usize, addr: u64, size: usize, dst_regs: &[String], depth: usize) -> Option<TracePath> {
        // 生成按优先级排序的搜索 patterns
        println!("[mem2mem]: 启发式搜索 0x{:x} ({} 字节)", addr, size);
        let search_patterns = ParsedInsn::gen_mem_read_patterns(addr, size);
        println!("{:#?}",search_patterns);

        
        // 按优先级依次尝试每个 pattern
        for (priority, pattern) in search_patterns.iter().enumerate() {
            self.debug_log(&format!("  [优先级 {}] {}: {}", priority + 1, pattern.description, pattern.pattern));
            
            if let Some(result) = self.search_with_pattern(line_num, pattern, addr, size, dst_regs, depth) {
                return Some(result);
            }
        }
        
        println!("❌ 所有搜索策略均未找到来源");
        None
    }

    /// 使用指定的 pattern 搜索匹配的 store 指令
    fn search_with_pattern(
        &mut self,
        line_num: usize,
        pattern: &crate::insn_analyzer::SearchPattern,
        addr: u64,
        size: usize,
        dst_regs: &[String],
        depth: usize,
    ) -> Option<TracePath> {
        let config = SearchConfig::new(pattern.pattern.clone()).with_regex(pattern.is_regex);
        let mut current_line = line_num;
        
        loop {
            let prev = self.service.find_prev(current_line, config.clone())?;
            
            let line_text = self.service.get_line_text(prev.line_number)?;
            let parsed = ParsedInsn::parse(&line_text);
            
            // 检查是否是store指令并获取信息
            if let Ok((src_regs, write_addr, write_size)) = parsed.get_store_info() {
                // 检查内存重叠
                if let Some((write_offset, overlap_size)) = ParsedInsn::check_memory_overlap(
                    addr, size, write_addr, write_size
                ) {
                    println!("  ✓ 找到匹配 [行 {}]: write[0x{:x}+{}:{}] -> read[0x{:x}:0x{:x}]", 
                             prev.line_number + 1,
                             write_addr,
                             write_offset,
                             write_offset + overlap_size,
                             addr,
                             addr + size as u64);
                    
                    self.debug_log(&format!("    {}", parsed.raw_text));

                    // 值校验
                    if !self.validate_store_value(line_num, prev.line_number, dst_regs, &src_regs, write_offset) {
                        current_line = prev.line_number;
                        continue;
                    }
                    
                    // 追踪源寄存器的特定字节范围
                    if let Some(result) = self.trace_source_register(
                        prev.line_number, &src_regs, write_offset, overlap_size, write_size, depth
                    ) {
                        return Some(result);
                    }

                    return self._trace_backward(prev.line_number, &format!("0x{:x}", addr), depth + 1);
                } else {
                    // 地址不匹配，继续向前搜索
                    self.debug_log(&format!("    [跳过] 地址不匹配: 0x{:x} vs 0x{:x}", write_addr, addr));
                    current_line = prev.line_number;
                    continue;
                }
            }
            
            // 解析失败，继续搜索
            current_line = prev.line_number;
        }
    }

    // 追踪内存写入（使用 ParsedInsn）
    fn trace_mem_write(&mut self, line_num: usize, src_regs: &[String], _addr: u64, _size: usize, depth: usize) -> Option<TracePath> {
        let src_reg = src_regs.first()?;
        
        let line_text = self.service.get_line_text(line_num)?;
        let parsed = ParsedInsn::parse(&line_text);
        
        parsed.read_regs.iter()
            .find(|(r, _)| r == src_reg)
            .and_then(|(_, value)| {
                let search_pattern = ParsedInsn::gen_reg_write_pattern(src_reg, value);
                println!("[regW] {}", search_pattern.pattern);
                
                let config = SearchConfig::new(search_pattern.pattern).with_regex(search_pattern.is_regex);
                self.find_and_trace(line_num, &config, src_reg, depth)
            })
    }

    // 追踪寄存器传递,从rr__到rw__（使用 InsnAnalyzer）
    fn trace_reg_transfer(&mut self, line_num: usize, reg: &str, _value: &str, depth: usize) -> Option<TracePath> {
        if ParsedInsn::is_zero_register(reg) {
            println!("终点: 零寄存器 {}", reg);
            return None;
        }
        
        let search_pattern = ParsedInsn::gen_reg_read_pattern(reg, _value);
        println!("[reg2reg]: {}", search_pattern.pattern);
        
        let config = SearchConfig::new(search_pattern.pattern).with_regex(search_pattern.is_regex);
        self.find_and_trace(line_num, &config, reg, depth)
    }

    // 追踪算术运算（使用 InsnAnalyzer）
    fn trace_arith_operation(&mut self, line_num: usize, regs: Vec<String>, depth: usize) -> Vec<TracePath> {
        // 解析寄存器值对
        let reg_values: Vec<(String, String)> = regs.iter()
            .filter_map(|pair| {
                pair.split_once('=')
                    .map(|(r, v)| (r.to_string(), v.to_string()))
            })
            .collect();
        
        // 生成搜索 patterns
        let patterns = ParsedInsn::gen_arith_patterns(&reg_values);
        
        println!("  → 开始追踪 {} 个源寄存器:", reg_values.len());
        
        reg_values.iter()
            .zip(patterns.iter())
            .enumerate()
            .filter(|(_, ((reg, val), _))| {
                // 跳过零寄存器和常量值
                !ParsedInsn::is_zero_register(reg) && !ParsedInsn::is_constant_value(val)
            })
            .filter_map(|(idx, ((reg, _), pattern))| {
                println!("  [分支 {}] 追踪寄存器: {}", idx + 1, reg);
                println!("    搜索模式: {}", pattern.pattern);
                let config = SearchConfig::new(pattern.pattern.clone()).with_regex(pattern.is_regex);
                
                self.service.find_prev(line_num, config)
                    .and_then(|prev| {
                        println!("    ✓ 找到 [行 {}]: {}", prev.line_number + 1,
                                 self.service.get_line_text(prev.line_number).unwrap_or_default());
                        self._trace_backward(prev.line_number, reg, depth + 1)
                    })
                    .or_else(|| {
                        println!("    ✗ 未找到来源");
                        None
                    })
            })
            .collect()
    }

    // 通用查找并追踪（通用执行函数）
    fn find_and_trace(&mut self, line_num: usize, config: &SearchConfig, target: &str, depth: usize) -> Option<TracePath> {
        self.service.find_prev(line_num, config.clone())
            .and_then(|prev| {
                println!("\t{}: {}", prev.line_number + 1, 
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

    // let file_path = std::path::PathBuf::from("/Users/teng/RustroverProjects/large-text-viewer/logs/record_01.csv");
    let file_path = std::path::PathBuf::from("/Users/bytedance/RustroverProjects/logs/record_01.csv");

    let reader = FileReader::new(file_path, encoding_rs::UTF_8)?;
    let service = SearchService::new(reader);

    let mut engine = TaintEngine::new(service).with_max_depth(15);

    println!("\n=== 追踪内存地址: ld__6cf01586a0_4 ===\n");
    // if let Some(trace) = engine.trace_backward(9028, "ld__6cf01586a0_4")? {
    if let Some(_trace) = engine.trace_backward(9218-1, "ld__6cf01586a8_8")? {
        // trace.print();
    }

    Ok(())
}

pub fn test_taint_overlap() -> anyhow::Result<()> {
    use large_text_core::file_reader::FileReader;

    // let file_path = std::path::PathBuf::from("/Users/teng/RustroverProjects/large-text-viewer/logs/record_01.csv");
    let file_path = std::path::PathBuf::from("/Users/bytedance/RustroverProjects/logs/record_01.csv");
    let reader = FileReader::new(file_path, encoding_rs::UTF_8)?;
    let service = SearchService::new(reader);

    let mut engine = TaintEngine::new(service)
        .with_max_depth(15)
        .with_debug(true);  // 启用调试模式

    println!("\n=== 追踪内存地址: ld__6cf01586a8_8 (测试内存重叠) ===\n");
    // ldr x21, [sp, #0xc08] 读取 0x6cf01586a8 (8字节)
    // 应该找到 str q0, [x19] 写入 0x6cf01586a0 (16字节)
    // if let Some(trace) = engine.trace_backward(11922-1, "st__6cf0157918_8")? {
    if let Some(trace) = engine.trace_backward(9217, "ld__6cf01586a8_8")? {
        println!("\n=== 追踪结果 ===\n");
        trace.print();
        println!("\n统计信息:");
        println!("  - 最大深度: {}", trace.max_depth());
        println!("  - 指令数量: {}", trace.count_instructions());
        
        println!("\n=== 开始渲染二叉树 ===\n");
        trace.render_as_tree();
    }

    Ok(())
}