// taint_engine.rs
use crate::search_service::{SearchService, SearchConfig};
use crate::insn_analyzer::{InsnType, ParsedInsn};
use crate::tree::{TreeNode, Tree};
use anyhow::Result;
use std::collections::HashSet;
use agf_render::{Graph, EdgeColor, layout, render_to_stdout};

const MAX_LINE_LENGTH: usize = 45;

pub struct TaintEngine {
    service: SearchService,
    max_depth: usize,
    visited: HashSet<usize>,
    debug: bool,
    current_byte_range: Option<(String, usize, usize)>,
}

#[derive(Debug, Clone)]
pub enum TraceType {
    MemToReg(String),
    RegToMem(String),
    RegToReg(String),
    Arith(Vec<String>),
    Constant,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct TaintTreeNode {
    pub line_num: usize,
    pub instruction: String,
    pub trace_type: TraceType,
    pub depth: usize,
    pub children: Vec<TaintTreeNode>,
    pub parsed_insn: Option<ParsedInsn>,
}

impl TaintTreeNode {
    fn new(line_num: usize, depth: usize) -> Self {
        Self {
            line_num,
            instruction: String::new(),
            trace_type: TraceType::Unknown,
            depth,
            children: Vec::new(),
            parsed_insn: None,
        }
    }



    fn init_from_service(&mut self, service: &mut SearchService) {
        if let Some(line_text) = service.get_line_text(self.line_num) {
            self.instruction = line_text.clone();
            self.parsed_insn = Some(ParsedInsn::parse(&line_text));
        }
    }

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
        
        println!("{}[{}] {} | {}", prefix, self.line_num + 1, type_str,
                 self.instruction.split(';').take(5).collect::<Vec<_>>().join(";"));
        
        for child in &self.children {
            child.print_with_indent(indent + 1);
        }
    }

    pub fn render(&self) {
        println!("\n=== 渲染追踪路径为二叉树 ===\n");
        let mut graph = Graph::new();
        self.add_to_graph(&mut graph);
        layout(&mut graph);
        render_to_stdout(&graph);
        println!("\n=== 二叉树渲染完成 ===\n");
    }

    fn add_to_graph(&self, graph: &mut Graph) -> usize {
        let display_text = self.format_for_graph();
        let node_id = graph.add_node("", &display_text);
        
        for (idx, child) in self.children.iter().enumerate() {
            let child_id = child.add_to_graph(graph);
            let color = if idx % 2 == 0 { EdgeColor::True } else { EdgeColor::False };
            graph.add_edge(node_id, child_id, color);
        }
        
        node_id
    }

    fn format_for_graph(&self) -> String {
        let parts: Vec<&str> = self.instruction.split(';').collect();
        let line_num = self.line_num + 1;
        let insn_name = parts.get(3).unwrap_or(&"").trim();
        let insn_opt = parts.get(4).unwrap_or(&"").trim();

        let mut mem_info = String::new();
        if let Some(parsed) = &self.parsed_insn {
            if let Some(addr) = parsed.mem_addr {
                mem_info = format!(" @0x{:x}", addr);
            }
        }
        
        let line = format!("{}:{} {}{}", line_num, insn_name, insn_opt, mem_info);
        Self::truncate_text(&line, MAX_LINE_LENGTH)
    }

    fn truncate_text(text: &str, max_len: usize) -> String {
        if text.chars().count() <= max_len {
            return text.to_string();
        }
        let truncated: String = text.chars().take(max_len - 3).collect();
        format!("{}...", truncated)
    }
}

impl TreeNode for TaintTreeNode {
    fn children_mut(&mut self) -> &mut Vec<Self> {
        &mut self.children
    }
    
    fn children(&self) -> &[Self] {
        &self.children
    }
    
    fn depth(&self) -> usize {
        self.depth
    }
}

#[derive(Debug, Clone)]
pub struct TaintTree {
    root: Option<TaintTreeNode>,
}

impl TaintTree {
    pub fn new() -> Self {
        Self {
            root: None,
        }
    }

    pub fn print(&self) {
        if let Some(root) = &self.root {
            root.print();
        }
    }

    pub fn render(&self) {
        if let Some(root) = &self.root {
            root.render();
        }
    }

    pub fn count_instructions(&self) -> usize {
        self.count_nodes()
    }
}

impl Tree<TaintTreeNode> for TaintTree {
    fn root(&self) -> Option<&TaintTreeNode> {
        self.root.as_ref()
    }

    fn root_mut(&mut self) -> Option<&mut TaintTreeNode> {
        self.root.as_mut()
    }

    fn set_root(&mut self, root: TaintTreeNode) {
        self.root = Some(root);
    }
}

impl Default for TaintTree {
    fn default() -> Self {
        Self::new()
    }
}

// TODO 新建shadow_mem；shadow_reg机制，隐线追踪mem区间


impl TaintEngine {
    pub fn new(service: SearchService) -> Self {
        Self {
            service,
            max_depth: 10,
            visited: HashSet::new(),
            debug: false,
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

    fn debug_log(&self, msg: &str) {
        if self.debug {
            println!("{}", msg);
        }
    }

    pub fn trace_backward(&mut self, start_line: usize, target: &str) -> Result<Option<TaintTree>> {
        self.visited.clear();
        println!("\n=== 开始反向追踪: {} 从行{} ===\n", target, start_line + 1);

        let mut tree = TaintTree::new();
        Ok(self._trace_backward_node(start_line, target, 0, &mut tree).map(|root| {
            tree.set_root(root);
            tree
        }))
    }

    fn _trace_backward_node(&mut self, line_num: usize, target: &str, depth: usize, tree: &mut TaintTree) -> Option<TaintTreeNode> {
        if depth >= self.max_depth { return None; }

        self.visited.insert(line_num);

        let mut node = TaintTreeNode::new(line_num, depth);
        node.init_from_service(&mut self.service);
        
        if depth == 0 {
            if let Some(line_text) = self.service.get_line_text(line_num) {
                println!("[target line]: {}", line_text);
            }
        }

        let parsed = node.parsed_insn.as_ref()?.clone();

        match parsed.insn_type {
            InsnType::Load => {
                let Ok((dst_regs, addr, size)) = parsed.get_load_info() else { return Some(node) };
                self.debug_log(&format!("  [Load] 目标寄存器: {:?}, target={}, byte_range={:?}", dst_regs, target, self.current_byte_range));
                
                let (adjusted_addr, adjusted_size) = ParsedInsn::calculate_adjusted_address(
                    &dst_regs, addr, size, target, &self.current_byte_range
                );
                
                node.trace_type = TraceType::MemToReg(format!("0x{:x}", adjusted_addr));
                if let Some(child) = self.trace_mem_read_node(line_num, adjusted_addr, adjusted_size, &dst_regs, depth, tree) {
                    node.add_child(child);
                }
            }
            InsnType::Store => {
                let Ok((src_regs, addr, size)) = parsed.get_store_info() else { return Some(node) };
                node.trace_type = TraceType::RegToMem(src_regs.join(","));
                if let Some(child) = self.trace_mem_write_node(line_num, &src_regs, addr, size, depth, tree) {
                    node.add_child(child);
                }
            }
            InsnType::Other => {
                if parsed.read_regs.is_empty() || parsed.write_regs.is_empty() {
                    println!("终点/常量");
                    node.trace_type = TraceType::Constant;
                } else {
                    println!("[Other] 数据转移不分支");
                    let (_src_reg, value) = &parsed.read_regs[0];
                    let dst_reg = _src_reg;
                    // let (dst_reg, _) = &parsed.write_regs[0];
                    node.trace_type = TraceType::RegToReg(dst_reg.clone());
                    if let Some(child) = self.trace_reg_transfer_node(line_num, dst_reg, value, depth, tree) {
                        node.add_child(child);
                    }
                }
            }
            InsnType::Arith => {
                if parsed.read_regs.is_empty() {
                    println!("终点/常量");
                    node.trace_type = TraceType::Constant;
                } else if parsed.read_regs.len() == 1 {
                    println!("[Reg Transfer/Branch]");
                    let (src_reg, value) = &parsed.read_regs[0];
                    node.trace_type = TraceType::RegToReg(src_reg.clone());
                    if let Some(child) = self.trace_reg_transfer_node(line_num, src_reg, value, depth, tree) {
                        node.add_child(child);
                    }
                } else {
                    println!("[Arith/Logic] 🌳 追踪分支: {} 个源寄存器", parsed.read_regs.len());
                    let src_regs: Vec<String> = parsed.read_regs.iter()
                        .map(|(reg, val)| format!("{}={}", reg, val))
                        .collect();
                    
                    node.trace_type = TraceType::Arith(parsed.read_regs.iter().map(|(r, _)| r.clone()).collect());
                    let children = self.trace_arith_operation_node(line_num, src_regs, depth, tree);
                    node.add_children(children);
                    
                    if !node.children.is_empty() {
                        println!("  ✓ 成功追踪 {} 个分支", node.children.len());
                    }
                }
            }

        }

        Some(node)
    }

    fn validate_store_value(
        &mut self,
        load_line_num: usize,
        store_line_num: usize,
        dst_regs: &[String],
        src_regs: &[String],
        write_offset: usize,
    ) -> bool {
        let Some(load_text) = self.service.get_line_text(load_line_num) else { return true; };
        let load_parsed = ParsedInsn::parse(&load_text);
        
        let Some(store_text) = self.service.get_line_text(store_line_num) else { return true; };
        let store_parsed = ParsedInsn::parse(&store_text);
        
        let target_reg = self.current_byte_range.as_ref()
            .map(|(reg, _, _)| reg.clone())
            .or_else(|| dst_regs.first().cloned())
            .unwrap_or_default();
        
        if target_reg.is_empty() { return true; }
        
        let Some((_, expected_value)) = load_parsed.write_regs.iter().find(|(reg, _)| reg == &target_reg) else { return true; };
        
        let src_reg_index = (write_offset / 8).min(src_regs.len().saturating_sub(1));
        let Some(src_reg) = src_regs.get(src_reg_index) else { return true; };
        
        if target_reg.chars().next() != src_reg.chars().next() {
            println!("  ℹ 寄存器类型不同 ({:?} vs {:?})，跳过值校验", target_reg.chars().next(), src_reg.chars().next());
            return true;
        }
        
        if let Some((_, actual_val)) = store_parsed.read_regs.iter().find(|(r, _)| r == src_reg) {
            if actual_val != expected_value {
                println!("  ⚠ 值不匹配: 期望 {}, 实际 {} (寄存器 {})", expected_value, actual_val, src_reg);
                println!("  → 继续搜索其他候选...");
                return false;
            }
            println!("  ✓ 值校验通过: {} = {}", src_reg, expected_value);
        }
        true
    }

    fn trace_source_register_node(
        &mut self,
        prev_line_num: usize,
        src_regs: &[String],
        write_offset: usize,
        overlap_size: usize,
        write_size: usize,
        depth: usize,
        tree: &mut TaintTree,
    ) -> Option<TaintTreeNode> {
        if src_regs.is_empty() { return None; }
        
        let src_reg_index = (write_offset / 8).min(src_regs.len().saturating_sub(1));
        let src_reg = &src_regs[src_reg_index];
        let reg_size = ParsedInsn::get_reg_size(src_reg);
        
        let reg_internal_offset = write_offset.saturating_sub(src_reg_index * reg_size);
        
        let old_byte_range = self.current_byte_range.clone();
        if write_offset > 0 || overlap_size < write_size {
            println!("  → 追踪 {} 的字节 [{}:{}]", src_reg, reg_internal_offset, reg_internal_offset + overlap_size);
            self.current_byte_range = Some((src_reg.clone(), reg_internal_offset, overlap_size));
        }
        
        let result = self._trace_backward_node(prev_line_num, src_reg, depth + 1, tree);
        self.current_byte_range = old_byte_range;
        
        result
    }

    fn trace_mem_read_node(&mut self, line_num: usize, addr: u64, size: usize, dst_regs: &[String], depth: usize, tree: &mut TaintTree) -> Option<TaintTreeNode> {
        println!("[mem2mem]: 启发式搜索 0x{:x} ({} 字节)", addr, size);
        let search_patterns = ParsedInsn::gen_mem_read_patterns(addr, size);
        println!("{:#?}", search_patterns);
        
        for (priority, pattern) in search_patterns.iter().enumerate() {
            self.debug_log(&format!("  [优先级 {}] {}: {}", priority + 1, pattern.description, pattern.pattern));
            
            if let Some(result) = self.search_with_pattern_node(line_num, pattern, addr, size, dst_regs, depth, tree) {
                return Some(result);
            }
        }
        
        println!("❌ 所有搜索策略均未找到来源");
        None
    }

    fn search_with_pattern_node(
        &mut self,
        line_num: usize,
        pattern: &crate::insn_analyzer::SearchPattern,
        addr: u64,
        size: usize,
        dst_regs: &[String],
        depth: usize,
        tree: &mut TaintTree,
    ) -> Option<TaintTreeNode> {
        let config = SearchConfig::new(pattern.pattern.clone()).with_regex(pattern.is_regex);
        let mut current_line = line_num;
        
        loop {
            let prev = self.service.find_prev(current_line, config.clone())?;
            current_line = prev.line_number;
            
            let line_text = self.service.get_line_text(current_line)?;
            let parsed = ParsedInsn::parse(&line_text);
            
            let Ok((src_regs, write_addr, write_size)) = parsed.get_store_info() else { continue; };
            
            let Some((write_offset, overlap_size)) = ParsedInsn::check_memory_overlap(addr, size, write_addr, write_size) else {
                self.debug_log(&format!("    [跳过] 地址不匹配: 0x{:x} vs 0x{:x}", write_addr, addr));
                continue;
            };
            
            println!("  ✓ 找到匹配 [行 {}]: write[0x{:x}+{}:{}] -> read[0x{:x}:0x{:x}]", 
                     current_line + 1, write_addr, write_offset, write_offset + overlap_size, addr, addr + size as u64);
            self.debug_log(&format!("    {}", parsed.raw_text));

            if !self.validate_store_value(line_num, current_line, dst_regs, &src_regs, write_offset) {
                continue;
            }
            
            if let Some(result) = self.trace_source_register_node(current_line, &src_regs, write_offset, overlap_size, write_size, depth, tree) {
                return Some(result);
            }

            return self._trace_backward_node(current_line, &format!("0x{:x}", addr), depth + 1, tree);
        }
    }

    fn trace_mem_write_node(&mut self, line_num: usize, src_regs: &[String], _addr: u64, _size: usize, depth: usize, tree: &mut TaintTree) -> Option<TaintTreeNode> {
        let src_reg = src_regs.first()?;
        let line_text = self.service.get_line_text(line_num)?;
        let parsed = ParsedInsn::parse(&line_text);
        
        let (_, value) = parsed.read_regs.iter().find(|(r, _)| r == src_reg)?;
        
        let search_pattern = ParsedInsn::gen_reg_write_pattern(src_reg, value);
        println!("[regW] {}", search_pattern.pattern);
        
        let config = SearchConfig::new(search_pattern.pattern).with_regex(search_pattern.is_regex);
        self.find_and_trace_node(line_num, &config, src_reg, depth, tree)
    }

    fn trace_reg_transfer_node(&mut self, line_num: usize, reg: &str, _value: &str, depth: usize, tree: &mut TaintTree) -> Option<TaintTreeNode> {
        if ParsedInsn::is_zero_register(reg) {
            println!("终点: 零寄存器 {}", reg);
            return None;
        }
        
        let search_pattern = ParsedInsn::gen_reg_read_pattern(reg, _value);
        println!("[reg2reg]: {}", search_pattern.pattern);
        
        let config = SearchConfig::new(search_pattern.pattern).with_regex(true);
        self.find_and_trace_node(line_num, &config, reg, depth, tree)
    }

    fn trace_arith_operation_node(&mut self, line_num: usize, regs: Vec<String>, depth: usize, tree: &mut TaintTree) -> Vec<TaintTreeNode> {
        let reg_values: Vec<(String, String)> = regs.iter()
            .filter_map(|pair| pair.split_once('=').map(|(r, v)| (r.to_string(), v.to_string())))
            .collect();
        
        let patterns = ParsedInsn::gen_arith_patterns(&reg_values);
        println!("  → 开始追踪 {} 个源寄存器:", reg_values.len());
        
        let mut result = Vec::new();
        for (idx, ((reg, val), pattern)) in reg_values.iter().zip(patterns.iter()).enumerate() {
            if ParsedInsn::is_zero_register(reg) || ParsedInsn::is_constant_value(val) { continue; }
            
            println!("  [分支 {}] 追踪寄存器: {}", idx + 1, reg);
            println!("    搜索模式: {}", pattern.pattern);
            
            let config = SearchConfig::new(pattern.pattern.clone()).with_regex(pattern.is_regex);
            
            if let Some(prev) = self.service.find_prev(line_num, config) {
                println!("    ✓ 找到 [行 {}]: {}", prev.line_number + 1, self.service.get_line_text(prev.line_number).unwrap_or_default());
                if let Some(node) = self._trace_backward_node(prev.line_number, reg, depth + 1, tree) {
                    result.push(node);
                }
            } else {
                println!("    ✗ 未找到来源");
            }
        }
        
        result
    }

    fn find_and_trace_node(&mut self, line_num: usize, config: &SearchConfig, target: &str, depth: usize, tree: &mut TaintTree) -> Option<TaintTreeNode> {
        let prev = self.service.find_prev(line_num, config.clone()).or_else(|| {
            println!("❌ 未找到来源");
            None
        })?;
        
        println!("\t{}: {}", prev.line_number + 1, self.service.get_line_text(prev.line_number).unwrap_or_default());
        self._trace_backward_node(prev.line_number, target, depth + 1, tree)
    }
}

pub fn test_taint_overlap() -> anyhow::Result<()> {
    use large_text_core::file_reader::FileReader;

    let file_path = std::path::PathBuf::from("/Users/bytedance/RustroverProjects/logs/record_01.csv");
    let reader = FileReader::new(file_path, encoding_rs::UTF_8)?;
    let service = SearchService::new(reader);

    let mut engine = TaintEngine::new(service)
        .with_max_depth(15)
        .with_debug(true);

    println!("\n=== 追踪内存地址: ld__6cf01586a8_8 ===\n");
    if let Some(tree) = engine.trace_backward(9217, "ld__6cf01586a8_8")? {
        println!("\n=== 追踪结果 ===\n");
        tree.print();
        println!("\n统计信息:");
        println!("  - 最大深度: {}", tree.max_depth());
        println!("  - 指令数量: {}", tree.count_instructions());
        
        println!("\n=== 开始渲染二叉树 ===\n");
        tree.render();
    }

    Ok(())
}
