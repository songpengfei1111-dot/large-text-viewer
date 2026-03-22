use crate::summery_analyzer::{AssemblyInstruction, AssemblyAnalyzer};
use std::collections::HashMap;

fn parse_hex(hex_str: &str) -> Option<u64> {
    let trimmed = hex_str.trim_start_matches("0x");
    u64::from_str_radix(trimmed, 16).ok()
}

// 删除 FunctionCall，不再需要
// #[derive(Debug, Clone)]
// pub struct FunctionCall {
//     pub call_line: usize,
//     pub call_addr: u64,
//     pub target_func_addr: u64,
//     pub ret_line: usize,
//     pub ret_addr: u64,
//     pub return_addr: u64,
// }

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

    pub fn build_call_tree(&self) -> CallTree {
        let mut nodes = Vec::new();
        let mut call_stack: Vec<u32> = Vec::new();
        let mut next_id = 0;

        // 1. 全局虚节点 (Node 0)
        nodes.push(CallTreeNode {
            id: next_id,
            func_addr: 0,
            call_line: 0,
            ret_line: usize::MAX,
            parent_id: None,
            children_ids: Vec::new(),
        });
        next_id += 1;

        // 2. 初始函数节点 (Node 1)
        let first_func_addr = self.instructions.first()
            .and_then(|instr| parse_hex(&instr.offset))
            .unwrap_or(0);
        nodes.push(CallTreeNode {
            id: next_id,
            func_addr: first_func_addr,
            call_line: 0,
            ret_line: usize::MAX,
            parent_id: Some(0),
            children_ids: Vec::new(),
        });
        nodes[0].children_ids.push(next_id);
        
        let mut current_id = next_id;
        next_id += 1;
        call_stack.push(0); // Node 0 入栈

        let mut blr_pending_pc: Option<u64> = None;

        for (i, instr) in self.instructions.iter().enumerate() {
            let pc = parse_hex(&instr.offset).unwrap_or(0);
            
            // Unidbg 拦截调用检测 (PC + 4)
            if let Some(blr_pc) = blr_pending_pc.take() {
                if pc != 0 {
                    nodes[current_id as usize].func_addr = pc; // 更新实际目标地址
                    if pc == blr_pc + 4 {
                        // 拦截调用，无函数体，自动闭合
                        if let Some(parent_id) = call_stack.pop() {
                            nodes[current_id as usize].ret_line = i.saturating_sub(1);
                            current_id = parent_id;
                        }
                    }
                } else {
                    blr_pending_pc = Some(blr_pc);
                }
            }

            // 处理指令，去掉前缀以适配不同的格式
            let opcode = instr.opcode.trim().to_lowercase();
            
            // 匹配所有分支跳转相关的调用 (bl, blr, b.eq, b.ne, 等等如果当作调用处理的话，这里严格按 bl/blr)
            // 根据 doc/traceui 里的实现，主要匹配 bl 和 blr
            if opcode.starts_with("bl") && !opcode.starts_with("blr") {
                let target = self.extract_call_target(instr).unwrap_or(0);
                let child_id = next_id;
                next_id += 1;
                
                nodes.push(CallTreeNode {
                    id: child_id,
                    func_addr: target,
                    call_line: i,
                    ret_line: usize::MAX,
                    parent_id: Some(current_id),
                    children_ids: Vec::new(),
                });
                nodes[current_id as usize].children_ids.push(child_id);
                call_stack.push(current_id);
                current_id = child_id;
            } else if opcode.starts_with("blr") {
                let target = self.extract_call_target(instr).unwrap_or(0);
                let child_id = next_id;
                next_id += 1;
                
                nodes.push(CallTreeNode {
                    id: child_id,
                    func_addr: target,
                    call_line: i,
                    ret_line: usize::MAX,
                    parent_id: Some(current_id),
                    children_ids: Vec::new(),
                });
                nodes[current_id as usize].children_ids.push(child_id);
                call_stack.push(current_id);
                current_id = child_id;
                
                blr_pending_pc = Some(pc);
            } else if opcode.starts_with("ret") {
                if let Some(parent_id) = call_stack.pop() {
                    nodes[current_id as usize].ret_line = i;
                    current_id = parent_id;
                }
            }
        }

        let total_lines = self.instructions.len();
        nodes[0].ret_line = total_lines.saturating_sub(1);
        // 强行闭合所有未返回的节点
        while let Some(parent_id) = call_stack.pop() {
            nodes[current_id as usize].ret_line = total_lines.saturating_sub(1);
            current_id = parent_id;
        }

        CallTree { nodes }
    }

    fn extract_call_target(&self, instr: &AssemblyInstruction) -> Option<u64> {
        let opcode = instr.opcode.trim().to_lowercase();
        if opcode.starts_with("bl") && !opcode.starts_with("blr") {
            let operands = &instr.operands;
            // 适配可能带括号或不带括号的情况
            if let Some(start) = operands.find('(') {
                if let Some(end) = operands.find(')') {
                    return parse_hex(&operands[start + 1..end]);
                }
            }
            // 尝试直接解析操作数
            return parse_hex(operands.trim());
        } else if opcode.starts_with("blr") {
            let reg_part = instr.operands.trim();
            let read_regs = &instr.read_regs;
            
            // 尝试匹配格式: "x8=0x..." 或 "x8=0x..."
            let search_pattern = format!("{}=", reg_part);
            if let Some(eq_pos) = read_regs.find(&search_pattern) {
                let mut val_start = eq_pos + search_pattern.len();
                if read_regs[val_start..].starts_with("0x") {
                    val_start += 2;
                }
                let val_end = read_regs[val_start..]
                    .find(|c: char| !c.is_ascii_hexdigit())
                    .map(|p| val_start + p)
                    .unwrap_or(read_regs.len());
                
                return parse_hex(&read_regs[val_start..val_end]);
            }
        }
        None
    }
}

impl CallTree {
    pub fn print(&self, max_depth: usize) {
        println!("=== 调用树 (最大深度: {}) ===", max_depth);
        self.print_tree(0, "", true, max_depth, 0);
    }

    pub fn print_merged(&self, max_depth: usize) {
        if self.nodes.is_empty() {
            println!("树为空");
            return;
        }
        
        println!("=== 调用树 (合并显示, 最大深度: {}) ===", max_depth);
        println!("根节点");
        self.print_node_merged(0, "", true, 0, max_depth);
    }

    fn print_node_merged(&self, node_id: u32, prefix: &str, is_last: bool, current_depth: usize, max_depth: usize) {
        if current_depth > max_depth {
            return;
        }

        let node = &self.nodes[node_id as usize];
        
        if node_id != 0 {
            let marker = if is_last { "└── " } else { "├── " };
            
            // 当前节点是否为合并节点（在外部传入时已处理好其显示内容）
            // 我们这里需要在打印子节点前对子节点进行合并
            // 合并逻辑: 对当前节点的所有子节点，按照 func_addr 进行分组聚合
            // 记录它们的调用范围和次数
        }
        
        // 提取并合并子节点
        let mut merged_children: Vec<(u64, Vec<u32>)> = Vec::new();
        // 保持原有的顺序（遇到新的func_addr或者连续的相同func_addr）
        // 如果想把所有相同的都合并，可以使用 HashMap，这里我们保留调用顺序，只合并连续相同的调用，或者合并所有相同的？
        // 用户要求："合并同一分支同一层级中相同函数的调用"，通常这意味着不关心顺序，只关心调用了哪些函数。
        // 这里我们把当前层级所有相同的 func_addr 合并在一起。
        
        let mut grouped_children: std::collections::HashMap<u64, Vec<u32>> = std::collections::HashMap::new();
        let mut order: Vec<u64> = Vec::new();

        for &child_id in &node.children_ids {
            let child = &self.nodes[child_id as usize];
            if !grouped_children.contains_key(&child.func_addr) {
                order.push(child.func_addr);
            }
            grouped_children.entry(child.func_addr).or_default().push(child_id);
        }

        for (i, &func_addr) in order.iter().enumerate() {
            let is_last_child = i == order.len() - 1;
            let group = &grouped_children[&func_addr];
            
            let count = group.len();
            
            // 收集所有的 call_line 和 ret_line
            let mut calls = Vec::new();
            for &id in group {
                let n = &self.nodes[id as usize];
                calls.push((n.call_line, n.ret_line));
            }
            
            let marker = if is_last_child { "└── " } else { "├── " };
            
            // 格式化输出: 0xXXXX (count次) [call1->ret1, call2->ret2...]
            // 为了避免太长，可以只显示部分或者简写
            if count == 1 {
                let n = &self.nodes[group[0] as usize];
                println!("{}{}0x{:x} ({},{})", prefix, marker, func_addr, n.call_line, n.ret_line);
            } else {
                let ranges: String = calls.iter()
                    .take(3) // 最多显示前3个
                    .map(|(c, r)| format!("{}->{}", c, r))
                    .collect::<Vec<_>>()
                    .join(", ");
                let more = if count > 3 { "..." } else { "" };
                println!("{}{}0x{:x} [共{}次] ({}{})", prefix, marker, func_addr, count, ranges, more);
            }

            // 递归打印子节点的子节点
            // 对于合并的节点，如果它们有自己的子树，我们如何展示？
            // 方案：把第一个代表性节点的子树展开，或者把所有合并节点的子节点都当成下一级？
            // 简单起见，如果合并了，我们把这组所有节点的子节点都归为下一层继续合并
            let next_prefix = format!("{}{}", prefix, if is_last_child { "    " } else { "│   " });
            
            if current_depth + 1 <= max_depth {
                // 构造一个虚拟节点或者直接在这里处理它们的子节点
                let mut all_grand_children = Vec::new();
                for &id in group {
                    all_grand_children.extend(self.nodes[id as usize].children_ids.iter().cloned());
                }
                
                if !all_grand_children.is_empty() {
                    self.print_merged_children(&all_grand_children, &next_prefix, current_depth + 1, max_depth);
                }
            }
        }
    }

    fn print_merged_children(&self, children_ids: &[u32], prefix: &str, current_depth: usize, max_depth: usize) {
        if current_depth > max_depth {
            return;
        }

        let mut grouped_children: std::collections::HashMap<u64, Vec<u32>> = std::collections::HashMap::new();
        let mut order: Vec<u64> = Vec::new();

        for &child_id in children_ids {
            let child = &self.nodes[child_id as usize];
            if !grouped_children.contains_key(&child.func_addr) {
                order.push(child.func_addr);
            }
            grouped_children.entry(child.func_addr).or_default().push(child_id);
        }

        for (i, &func_addr) in order.iter().enumerate() {
            let is_last_child = i == order.len() - 1;
            let group = &grouped_children[&func_addr];
            
            let count = group.len();
            
            let mut calls = Vec::new();
            for &id in group {
                let n = &self.nodes[id as usize];
                calls.push((n.call_line, n.ret_line));
            }
            
            let marker = if is_last_child { "└── " } else { "├── " };
            
            if count == 1 {
                let n = &self.nodes[group[0] as usize];
                println!("{}{}0x{:x} ({},{})", prefix, marker, func_addr, n.call_line, n.ret_line);
            } else {
                let ranges: String = calls.iter()
                    .take(3)
                    .map(|(c, r)| format!("{}->{}", c, r))
                    .collect::<Vec<_>>()
                    .join(", ");
                let more = if count > 3 { "..." } else { "" };
                println!("{}{}0x{:x} [共{}次] ({}{})", prefix, marker, func_addr, count, ranges, more);
            }

            let next_prefix = format!("{}{}", prefix, if is_last_child { "    " } else { "│   " });
            
            if current_depth + 1 <= max_depth {
                let mut all_grand_children = Vec::new();
                for &id in group {
                    all_grand_children.extend(self.nodes[id as usize].children_ids.iter().cloned());
                }
                
                if !all_grand_children.is_empty() {
                    self.print_merged_children(&all_grand_children, &next_prefix, current_depth + 1, max_depth);
                }
            }
        }
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

pub fn print_call_summary(call_tree: &CallTree) {
    println!("=== 基于正向扫描的函数调用分析 ===");
    let nodes = &call_tree.nodes;
    
    // nodes.len() 包含 1个全局虚节点 和 1个初始函数节点，所以实际调用数为 len - 2
    let actual_calls = nodes.len().saturating_sub(2);
    println!("总函数调用数: {}", actual_calls);
    
    let mut func_call_count: HashMap<u64, usize> = HashMap::new();
    for node in nodes.iter().skip(2) {
        *func_call_count.entry(node.func_addr).or_insert(0) += 1;
    }
    
    println!("\n被调用函数统计 (前20个):");
    let mut sorted_funcs: Vec<_> = func_call_count.iter().collect();
    sorted_funcs.sort_by(|a, b| b.1.cmp(a.1));
    
    for (i, (&addr, &count)) in sorted_funcs.iter().take(20).enumerate() {
        println!("  {}. 0x{:x}: {} 次", i + 1, addr, count);
    }
    
    println!("\n前20个函数调用详情:");
    for (i, node) in nodes.iter().skip(2).take(20).enumerate() {
        println!("  {}. 调用行: {} → 函数: 0x{:x} → 返回行: {}",
            i + 1, node.call_line, node.func_addr, node.ret_line);
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
            
            println!("\n调用树结构 (合并显示, 深度限制为 10):");
            call_tree.print_merged(51);
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
                let call_tree = analyzer.build_call_tree();
                print_call_summary(&call_tree);
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
                
                println!("=== 构建调用树 ===");
                let call_tree = analyzer.build_call_tree();
                
                println!("总节点数: {}", call_tree.get_node_count());
                println!("最大深度: {}", call_tree.get_max_depth());
                
                println!("\n调用树结构 (合并显示, 深度限制为 10):");
                call_tree.print_merged(10);
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
                
                let call_tree = analyzer.build_call_tree();
                
                println!("=== 测试根据行号获取调用上下文 ===");
                
                if call_tree.nodes.len() > 2 {
                    // test_line 取一个合理范围内的值
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
