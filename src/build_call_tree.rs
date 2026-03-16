use crate::summery_analyzer::{AssemblyInstruction, AssemblyAnalyzer};
use std::collections::HashMap;

pub fn filter_ret_line_numbers(instructions: &[AssemblyInstruction]) -> Vec<usize> {
    instructions
        .iter()
        .enumerate()
        .filter(|(_, instr)| instr.opcode.starts_with("ret"))
        .map(|(i, _)| i + 1)
        .collect()
}

#[derive(Debug, Clone)]
pub struct FunctionCall {
    pub call_line: usize,
    pub call_addr: u64,
    pub target_func_addr: u64,
    pub ret_line: usize,
    pub ret_addr: u64,
    pub return_addr: u64,
}

#[derive(Debug, Clone)]
pub struct CallTreeNode {
    pub id: u32,
    pub func_addr: u64,
    pub call_line: usize,
    pub ret_line: usize,
    pub parent_id: Option<u32>,
    pub children_ids: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct CallTree {
    pub nodes: Vec<CallTreeNode>,
}

#[derive(Debug, Clone)]
pub struct RetBasedCallTreeBuilder {
    instructions: Vec<AssemblyInstruction>,
    pub function_calls: Vec<FunctionCall>,
}

impl RetBasedCallTreeBuilder {
    pub fn new(instructions: Vec<AssemblyInstruction>) -> Self {
        Self {
            instructions,
            function_calls: Vec::new(),
        }
    }

    pub fn build(&mut self) {
        let ret_lines = filter_ret_line_numbers(&self.instructions);
        
        for &ret_line in &ret_lines {
            let ret_idx = ret_line - 1;
            
            if let Some(ret_instr) = self.instructions.get(ret_idx) {
                if let Ok(ret_addr) = u64::from_str_radix(&ret_instr.full_addr, 16) {
                    if let Some(next_instr) = self.instructions.get(ret_idx + 1) {
                        if let Ok(return_addr) = u64::from_str_radix(&next_instr.full_addr, 16) {
                            let call_addr = return_addr - 4;
                            
                            if let Some((call_line, call_instr)) = self.find_call_instruction_before(call_addr, ret_idx) {
                                if let Some(target_func_addr) = self.extract_call_target(&call_instr) {
                                    self.function_calls.push(FunctionCall {
                                        call_line,
                                        call_addr,
                                        target_func_addr,
                                        ret_line,
                                        ret_addr,
                                        return_addr,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn find_call_instruction_before(&self, target_addr: u64, before_idx: usize) -> Option<(usize, AssemblyInstruction)> {
        for i in (0..before_idx).rev() {
            if let Some(instr) = self.instructions.get(i) {
                if let Ok(addr) = u64::from_str_radix(&instr.full_addr, 16) {
                    if addr == target_addr {
                        if instr.opcode.starts_with("bl") || instr.opcode.starts_with("blr") {
                            return Some((i + 1, instr.clone()));
                        }
                    }
                }
            }
        }
        None
    }

    fn extract_call_target(&self, instr: &AssemblyInstruction) -> Option<u64> {
        if instr.opcode.starts_with("bl") && !instr.opcode.starts_with("blr") {
            let operands = &instr.operands;
            if let Some(start_paren) = operands.find('(') {
                if let Some(end_paren) = operands.find(')') {
                    let offset_str = &operands[start_paren + 1..end_paren];
                    return u64::from_str_radix(offset_str, 16).ok();
                }
            }
        } else if instr.opcode.starts_with("blr") {
            let reg_part = &instr.operands;
            let read_regs = &instr.read_regs;
            
            if let Some(eq_pos) = read_regs.find(&format!("{}=", reg_part)) {
                let val_start = eq_pos + reg_part.len() + 1;
                let val_start = if read_regs[val_start..].starts_with("0x") {
                    val_start + 2
                } else {
                    val_start
                };
                let val_end = read_regs[val_start..]
                    .find(|c: char| !c.is_ascii_hexdigit())
                    .map(|p| val_start + p)
                    .unwrap_or(read_regs.len());
                if let Ok(val) = u64::from_str_radix(&read_regs[val_start..val_end], 16) {
                    return Some(val);
                }
            }
        }
        None
    }

    pub fn print_summary(&self) {
        println!("=== 基于 ret 指令的函数调用分析 ===");
        println!("总函数调用数: {}", self.function_calls.len());
        
        let mut func_call_count: HashMap<u64, usize> = HashMap::new();
        for call in &self.function_calls {
            *func_call_count.entry(call.target_func_addr).or_insert(0) += 1;
        }
        
        println!("\n被调用函数统计 (前20个):");
        let mut sorted_funcs: Vec<_> = func_call_count.iter().collect();
        sorted_funcs.sort_by(|a, b| b.1.cmp(a.1));
        
        for (i, (&addr, &count)) in sorted_funcs.iter().take(20).enumerate() {
            println!("  {}. 0x{:x}: {} 次", i + 1, addr, count);
        }
        
        println!("\n前20个函数调用详情:");
        for (i, call) in self.function_calls.iter().take(20).enumerate() {
            println!("  {}. 调用行: {} → 函数: 0x{:x} → 返回行: {}",
                i + 1, call.call_line, call.target_func_addr, call.ret_line);
        }
    }

    pub fn build_call_tree(&self) -> CallTree {
        let mut sorted_calls = self.function_calls.clone();
        sorted_calls.sort_by_key(|call| call.call_line);

        let mut nodes = Vec::new();
        let mut call_stack: Vec<u32> = Vec::new();
        let mut next_id = 0;

        let root = CallTreeNode {
            id: next_id,
            func_addr: 0,
            call_line: 0,
            ret_line: usize::MAX,
            parent_id: None,
            children_ids: Vec::new(),
        };
        nodes.push(root);
        next_id += 1;
        call_stack.push(0);

        for call in &sorted_calls {
            while let Some(&current_id) = call_stack.last() {
                let current_node = &nodes[current_id as usize];
                if current_node.ret_line < call.call_line {
                    call_stack.pop();
                } else {
                    break;
                }
            }

            let parent_id = call_stack.last().copied();
            let child_id = next_id;
            next_id += 1;

            let child = CallTreeNode {
                id: child_id,
                func_addr: call.target_func_addr,
                call_line: call.call_line,
                ret_line: call.ret_line,
                parent_id,
                children_ids: Vec::new(),
            };
            nodes.push(child);

            if let Some(parent_id) = parent_id {
                nodes[parent_id as usize].children_ids.push(child_id);
            }

            call_stack.push(child_id);
        }

        CallTree { nodes }
    }
}

impl CallTree {
    pub fn print(&self, max_depth: usize) {
        println!("=== 调用树 (最大深度: {}) ===", max_depth);
        self.print_tree(0, "", true, max_depth, 0);
    }

    fn print_tree(&self, node_id: u32, prefix: &str, is_last: bool, max_depth: usize, current_depth: usize) {
        if current_depth > max_depth {
            return;
        }

        let node = &self.nodes[node_id as usize];
        
        if node_id == 0 {
            println!("根节点");
        } else {
            let node_prefix = if is_last { "└── " } else { "├── " };
            println!("{}{}0x{:x} ({},{})",
                prefix, node_prefix, node.func_addr, node.call_line, node.ret_line);
        }

        let children = &node.children_ids;
        let child_count = children.len();
        for (i, &child_id) in children.iter().enumerate() {
            let is_last_child = i == child_count - 1;
            let new_prefix = if node_id == 0 {
                "".to_string()
            } else if is_last {
                format!("{}    ", prefix)
            } else {
                format!("{}│   ", prefix)
            };
            self.print_tree(child_id, &new_prefix, is_last_child, max_depth, current_depth + 1);
        }
    }

    pub fn get_node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn get_max_depth(&self) -> usize {
        self.get_max_depth_recursive(0, 0)
    }

    fn get_max_depth_recursive(&self, node_id: u32, current_depth: usize) -> usize {
        let node = &self.nodes[node_id as usize];
        let mut max_depth = current_depth;

        for &child_id in &node.children_ids {
            let child_depth = self.get_max_depth_recursive(child_id, current_depth + 1);
            if child_depth > max_depth {
                max_depth = child_depth;
            }
        }

        max_depth
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_ret_from_record_01() {
        match AssemblyAnalyzer::new("logs/record_01.csv") {
            Ok(analyzer) => {
                let instructions = analyzer.instructions();
                let ret_line_numbers = filter_ret_line_numbers(instructions);
                
                println!("=== record_01.csv 中的 ret 指令行号 ===");
                println!("总 ret 指令数量: {}", ret_line_numbers.len());
                
                println!("\n前 20 个 ret 指令的行号:");
                for &line_num in ret_line_numbers.iter().take(20) {
                    if let Some(instr) = instructions.get(line_num - 1) {
                        println!("  行号 {}: {} {}", line_num, instr.opcode, instr.operands);
                    }
                }
            }
            Err(e) => {
                eprintln!("错误: 无法读取文件: {}", e);
            }
        }
    }

    #[test]
    fn test_ret_based_call_tree() {
        match AssemblyAnalyzer::new("logs/record_01.csv") {
            Ok(analyzer) => {
                let instructions = analyzer.instructions().to_vec();
                let mut builder = RetBasedCallTreeBuilder::new(instructions);
                builder.build();
                builder.print_summary();
            }
            Err(e) => {
                eprintln!("错误: 无法读取文件: {}", e);
            }
        }
    }

    #[test]
    fn test_build_call_tree() {
        match AssemblyAnalyzer::new("logs/record_01.csv") {
            Ok(analyzer) => {
                let instructions = analyzer.instructions().to_vec();
                let mut builder = RetBasedCallTreeBuilder::new(instructions);
                builder.build();
                
                println!("=== 构建调用树 ===");
                let call_tree = builder.build_call_tree();
                
                println!("总节点数: {}", call_tree.get_node_count());
                println!("最大深度: {}", call_tree.get_max_depth());
                
                println!("\n调用树结构 (深度限制为 5):");
                call_tree.print(5);
            }
            Err(e) => {
                eprintln!("错误: 无法读取文件: {}", e);
            }
        }
    }
}



pub fn test_build_call_tree() {
    match AssemblyAnalyzer::new("logs/record_01.csv") {
        Ok(analyzer) => {
            let instructions = analyzer.instructions().to_vec();
            let mut builder = RetBasedCallTreeBuilder::new(instructions);
            builder.build();

            println!("=== 构建调用树 ===");
            let call_tree = builder.build_call_tree();

            println!("总节点数: {}", call_tree.get_node_count());
            println!("最大深度: {}", call_tree.get_max_depth());

            println!("\n调用树结构 (深度限制为 5):");
            call_tree.print(5);
        }
        Err(e) => {
            eprintln!("错误: 无法读取文件: {}", e);
        }
    }
}