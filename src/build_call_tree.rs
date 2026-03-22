use crate::summery_analyzer::{AssemblyInstruction, AssemblyAnalyzer};
use std::collections::HashMap;

fn parse_hex(hex_str: &str) -> Option<u64> {
    let trimmed = hex_str.trim_start_matches("0x");
    u64::from_str_radix(trimmed, 16).ok()
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
    nodes: Vec<CallTreeNode>,
}

#[derive(Debug, Clone)]
pub struct CallContext {
    pub current_node: CallTreeNode,
    pub call_chain: Vec<CallTreeNode>,
}

pub struct FunctionCallAnalyzer {
    instructions: Vec<AssemblyInstruction>,
    addr_map: HashMap<u64, usize>,
}

impl FunctionCallAnalyzer {
    pub fn new(instructions: Vec<AssemblyInstruction>) -> Self {
        let mut addr_map = HashMap::new();
        for (i, instr) in instructions.iter().enumerate() {
            if let Some(addr) = parse_hex(&instr.offset) {
                addr_map.insert(addr, i);
            }
        }
        Self {
            instructions,
            addr_map,
        }
    }

    pub fn analyze(&self) -> (Vec<FunctionCall>, Option<usize>) {
        let mut function_calls = Vec::new();
        let mut first_unmatched_ret = None;

        for (ret_idx, ret_instr) in self.instructions.iter().enumerate() {
            if !ret_instr.opcode.starts_with("ret") {
                continue;
            }

            let Some(ret_addr) = parse_hex(&ret_instr.offset) else { continue };
            let Some(next_instr) = self.instructions.get(ret_idx + 1) else { continue };
            let Some(return_addr) = parse_hex(&next_instr.offset) else { continue };
            let call_addr = return_addr - 4;

            let Some((call_line, call_instr)) = self.find_call_instruction(call_addr, ret_idx) else {
                eprintln!("err :[{:x},{:x}]", return_addr, call_addr);
                if first_unmatched_ret.is_none() {
                    first_unmatched_ret = Some(ret_idx + 1);
                }
                continue;
            };

            // println!("[{:x},{:x},{}]", return_addr, call_addr, call_line);

            let Some(target_func_addr) = self.extract_call_target(&call_instr) else { continue };

            function_calls.push(FunctionCall {
                call_line,
                call_addr,
                target_func_addr,
                ret_line: ret_idx + 1,
                ret_addr,
                return_addr,
            });
        }

        (function_calls, first_unmatched_ret)
    }

    pub fn build_call_tree(&self) -> CallTree {
        let (function_calls, first_unmatched_ret) = self.analyze();
        let first_func_addr = self.instructions.first()
            .and_then(|instr| parse_hex(&instr.offset))
            .unwrap_or(0);
        Self::build_tree_from_calls(&function_calls, first_func_addr, first_unmatched_ret)
    }

    pub fn build_tree_from_calls(function_calls: &[FunctionCall], first_func_addr: u64, node1_ret_line: Option<usize>) -> CallTree {
        let mut sorted_calls = function_calls.to_vec();
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

        let first_func = CallTreeNode {
            id: next_id,
            func_addr: first_func_addr,
            call_line: 0,
            ret_line: node1_ret_line.unwrap_or(usize::MAX),
            parent_id: Some(0),
            children_ids: Vec::new(),
        };
        nodes[0].children_ids.push(next_id);
        nodes.push(first_func);
        next_id += 1;

        call_stack.push(0);
        call_stack.push(1);

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

    fn find_call_instruction(&self, target_addr: u64, before_idx: usize) -> Option<(usize, &AssemblyInstruction)> {
        if let Some(&idx) = self.addr_map.get(&target_addr) {
            if idx < before_idx {
                let instr = &self.instructions[idx];
                if instr.opcode.starts_with("bl") || instr.opcode.starts_with("blr") {
                    return Some((idx + 1, instr));
                }
            }
        }

        for i in (0..before_idx).rev() {
            let instr = &self.instructions[i];
            if let Some(addr) = parse_hex(&instr.offset) {
                if addr == target_addr && (instr.opcode.starts_with("bl") || instr.opcode.starts_with("blr")) {
                    return Some((i + 1, instr));
                }
            }
        }
        None
    }

    fn extract_call_target(&self, instr: &AssemblyInstruction) -> Option<u64> {
        if instr.opcode.starts_with("bl") && !instr.opcode.starts_with("blr") {
            let operands = &instr.operands;
            let start = operands.find('(')?;
            let end = operands.find(')')?;
            return parse_hex(&operands[start + 1..end]);
        } else if instr.opcode.starts_with("blr") {
            let reg_part = &instr.operands;
            let read_regs = &instr.read_regs;

            let eq_pos = read_regs.find(&format!("{}=", reg_part))?;
            let mut val_start = eq_pos + reg_part.len() + 1;
            if read_regs[val_start..].starts_with("0x") {
                val_start += 2;
            }
            let val_end = read_regs[val_start..]
                .find(|c: char| !c.is_ascii_hexdigit())
                .map(|p| val_start + p)
                .unwrap_or(read_regs.len());

            return parse_hex(&read_regs[val_start..val_end]);
        }
        None
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

    pub fn get_call_context_by_line(&self, line_number: usize) -> Option<CallContext> {
        let mut deepest_node_id: Option<u32> = None;
        let mut max_depth = 0;

        for (id, node) in self.nodes.iter().enumerate() {
            if node.call_line <= line_number && line_number <= node.ret_line {
                let depth = self.get_node_depth(id as u32);
                if depth > max_depth {
                    max_depth = depth;
                    deepest_node_id = Some(id as u32);
                }
            }
        }

        deepest_node_id.map(|node_id| {
            let current_node = self.nodes[node_id as usize].clone();
            let call_chain = self.get_call_chain(node_id);
            CallContext {
                current_node,
                call_chain,
            }
        })
    }

    fn get_node_depth(&self, node_id: u32) -> usize {
        let mut depth = 0;
        let mut current_id = Some(node_id);

        while let Some(id) = current_id {
            let node = &self.nodes[id as usize];
            current_id = node.parent_id;
            if current_id.is_some() {
                depth += 1;
            }
        }

        depth
    }

    fn get_call_chain(&self, node_id: u32) -> Vec<CallTreeNode> {
        let mut chain = Vec::new();
        let mut current_id = Some(node_id);

        while let Some(id) = current_id {
            let node = self.nodes[id as usize].clone();
            chain.push(node);
            current_id = self.nodes[id as usize].parent_id;
        }

        chain.reverse();
        chain
    }
}

pub fn print_call_summary(function_calls: &[FunctionCall]) {
    println!("=== 基于 ret 指令的函数调用分析 ===");
    println!("总函数调用数: {}", function_calls.len());

    let mut func_call_count: HashMap<u64, usize> = HashMap::new();
    for call in function_calls {
        *func_call_count.entry(call.target_func_addr).or_insert(0) += 1;
    }

    println!("\n被调用函数统计 (前20个):");
    let mut sorted_funcs: Vec<_> = func_call_count.iter().collect();
    sorted_funcs.sort_by(|a, b| b.1.cmp(a.1));

    for (i, (&addr, &count)) in sorted_funcs.iter().take(20).enumerate() {
        println!("  {}. 0x{:x}: {} 次", i + 1, addr, count);
    }

    println!("\n前20个函数调用详情:");
    for (i, call) in function_calls.iter().take(20).enumerate() {
        println!("  {}. 调用行: {} → 函数: 0x{:x} → 返回行: {}",
                 i + 1, call.call_line, call.target_func_addr, call.ret_line);
    }
}

pub fn test_build_call_tree() {
    match AssemblyAnalyzer::new("logs/record_01.csv") {
        Ok(analyzer) => {
            let instructions = analyzer.instructions().to_vec();
            let analyzer = FunctionCallAnalyzer::new(instructions);

            println!("=== 构建调用树 ===");
            let call_tree = analyzer.build_call_tree();

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ret_based_call_tree() {
        match AssemblyAnalyzer::new("logs/record_01.csv") {
            Ok(analyzer) => {
                let instructions = analyzer.instructions().to_vec();
                let analyzer = FunctionCallAnalyzer::new(instructions);
                let (function_calls, _) = analyzer.analyze();
                print_call_summary(&function_calls);
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
                let analyzer = FunctionCallAnalyzer::new(instructions);
                let (function_calls, first_unmatched_ret) = analyzer.analyze();
                let first_func_addr = analyzer.instructions.first()
                    .and_then(|instr| parse_hex(&instr.offset))
                    .unwrap_or(0);

                println!("=== 构建调用树 ===");
                let call_tree = FunctionCallAnalyzer::build_tree_from_calls(&function_calls, first_func_addr, first_unmatched_ret);

                println!("总节点数: {}", call_tree.get_node_count());
                println!("最大深度: {}", call_tree.get_max_depth());

                println!("\n调用树结构 (深度限制为 5):");
                call_tree.print(10);
            }
            Err(e) => {
                eprintln!("错误: 无法读取文件: {}", e);
            }
        }
    }

    #[test]
    fn test_get_call_context_by_line() {
        match AssemblyAnalyzer::new("logs/record_01.csv") {
            Ok(analyzer) => {
                let instructions = analyzer.instructions().to_vec();
                let analyzer = FunctionCallAnalyzer::new(instructions);
                let (function_calls, first_unmatched_ret) = analyzer.analyze();
                let first_func_addr = analyzer.instructions.first()
                    .and_then(|instr| parse_hex(&instr.offset))
                    .unwrap_or(0);

                let call_tree = FunctionCallAnalyzer::build_tree_from_calls(&function_calls, first_func_addr, first_unmatched_ret);

                println!("=== 测试根据行号获取调用上下文 ===");

                if !function_calls.is_empty() {
                    let first_call = &function_calls[0];
                    // let test_line = first_call.call_line + 1;
                    let test_line = 6666;

                    println!("测试行号: {}", test_line);

                    if let Some(context) = call_tree.get_call_context_by_line(test_line) {
                        println!("当前函数: 0x{:x}", context.current_node.func_addr);
                        println!("调用行: {}, 返回行: {}", context.current_node.call_line, context.current_node.ret_line);
                        println!("调用链长度: {}", context.call_chain.len());
                        println!("调用链:");
                        for (i, node) in context.call_chain.iter().enumerate() {
                            if i == 0 {
                                println!("  [{}] 根节点", i);
                            } else {
                                println!("  [{}] 0x{:x} ({},{})", i, node.func_addr, node.call_line, node.ret_line);
                            }
                        }
                    } else {
                        println!("未找到该行号的调用上下文");
                    }
                } else {
                    println!("没有找到函数调用，无法测试");
                }
            }
            Err(e) => {
                eprintln!("错误: 无法读取文件: {}", e);
            }
        }
    }
}
