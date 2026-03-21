use std::collections::{HashMap, HashSet};
use crate::insn_analyzer::ParsedInsn;
use crate::search_service::SearchService;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DepNode {
    Line(usize),
    LineHalf1(usize),
    LineHalf2(usize),
}

impl DepNode {
    pub fn line(&self) -> usize {
        match self {
            DepNode::Line(l) => *l,
            DepNode::LineHalf1(l) => *l,
            DepNode::LineHalf2(l) => *l,
        }
    }
}

#[derive(Default, Clone)]
pub struct LineDeps {
    pub normal: Vec<DepNode>,
    pub half1: Vec<DepNode>,
    pub half2: Vec<DepNode>,
}

pub struct ScanState {
    pub reg_last_def: HashMap<String, DepNode>,
    pub mem_last_def: HashMap<u64, DepNode>,
    pub deps: Vec<LineDeps>,
    pub line_count: usize,
}

impl ScanState {
    pub fn new(capacity: usize) -> Self {
        Self {
            reg_last_def: HashMap::new(),
            mem_last_def: HashMap::new(),
            deps: vec![LineDeps::default(); capacity],
            line_count: 0,
        }
    }
}

fn normalize_reg(reg: &str) -> String {
    ParsedInsn::normalize_reg(reg)
}

fn push_unique(vec: &mut Vec<DepNode>, val: DepNode) {
    if !vec.contains(&val) {
        vec.push(val);
    }
}

pub fn scan_pass(service: &mut SearchService) -> anyhow::Result<ScanState> {
    let total_lines = service.total_lines();
    let mut state = ScanState::new(total_lines);

    for line_num in 0..total_lines {
        if let Some(line_text) = service.get_line_text(line_num) {
            let parsed = ParsedInsn::parse(&line_text);
            let is_ldp = parsed.opcode.starts_with("ldp");
            let is_stp = parsed.opcode.starts_with("stp");
            let is_pair = is_ldp || is_stp;

            let mut line_deps = LineDeps::default();

            // 1. 查找寄存器 Use 依赖
            if is_stp && parsed.read_regs.len() >= 2 {
                // stp 的源寄存器拆分
                let reg1 = normalize_reg(&parsed.read_regs[0].0);
                let reg2 = normalize_reg(&parsed.read_regs[1].0);
                
                if let Some(&def_node) = state.reg_last_def.get(&reg1) {
                    push_unique(&mut line_deps.half1, def_node);
                }
                if let Some(&def_node) = state.reg_last_def.get(&reg2) {
                    push_unique(&mut line_deps.half2, def_node);
                }
                // 如果有第3个读寄存器(比如基址寄存器)，放入 shared(normal)
                for i in 2..parsed.read_regs.len() {
                    let r = normalize_reg(&parsed.read_regs[i].0);
                    if let Some(&def_node) = state.reg_last_def.get(&r) {
                        push_unique(&mut line_deps.normal, def_node);
                    }
                }
            } else {
                // 普通指令读取寄存器
                for (reg_name, _) in &parsed.read_regs {
                    if ParsedInsn::is_zero_register(reg_name) { continue; }
                    let norm_reg = normalize_reg(reg_name);
                    if let Some(&def_node) = state.reg_last_def.get(&norm_reg) {
                        push_unique(&mut line_deps.normal, def_node);
                    }
                }
            }

            // 2. 查找内存 Use 依赖 (Load 指令)
            if parsed.mem_access_type == 1 { // Load
                if let (Some(addr), Some(size)) = (parsed.mem_addr, parsed.mem_size) {
                    if is_ldp && size > 1 {
                        let half_size = size / 2;
                        // half1 mem deps
                        for offset in 0..half_size as u64 {
                            if let Some(&def_node) = state.mem_last_def.get(&(addr + offset)) {
                                push_unique(&mut line_deps.half1, def_node);
                            }
                        }
                        // half2 mem deps
                        for offset in half_size as u64..size as u64 {
                            if let Some(&def_node) = state.mem_last_def.get(&(addr + offset)) {
                                push_unique(&mut line_deps.half2, def_node);
                            }
                        }
                    } else {
                        // 普通 Load
                        for offset in 0..size as u64 {
                            if let Some(&def_node) = state.mem_last_def.get(&(addr + offset)) {
                                push_unique(&mut line_deps.normal, def_node);
                            }
                        }
                    }
                }
            }

            // 3. 更新寄存器 Def
            if is_ldp && parsed.write_regs.len() >= 2 {
                let reg1 = normalize_reg(&parsed.write_regs[0].0);
                let reg2 = normalize_reg(&parsed.write_regs[1].0);
                state.reg_last_def.insert(reg1, DepNode::LineHalf1(line_num));
                state.reg_last_def.insert(reg2, DepNode::LineHalf2(line_num));
                
                // 写回的基址寄存器等
                for i in 2..parsed.write_regs.len() {
                    let r = normalize_reg(&parsed.write_regs[i].0);
                    state.reg_last_def.insert(r, DepNode::Line(line_num));
                }
            } else {
                for (reg_name, _) in &parsed.write_regs {
                    if ParsedInsn::is_zero_register(reg_name) { continue; }
                    let norm_reg = normalize_reg(reg_name);
                    state.reg_last_def.insert(norm_reg, DepNode::Line(line_num));
                }
            }

            // 4. 更新内存 Def (Store)
            if parsed.mem_access_type == 2 { // Store
                if let (Some(addr), Some(size)) = (parsed.mem_addr, parsed.mem_size) {
                    if is_stp && size > 1 {
                        let half_size = size / 2;
                        // half1 bytes -> LineHalf1
                        for offset in 0..half_size as u64 {
                            state.mem_last_def.insert(addr + offset, DepNode::LineHalf1(line_num));
                        }
                        // half2 bytes -> LineHalf2
                        for offset in half_size as u64..size as u64 {
                            state.mem_last_def.insert(addr + offset, DepNode::LineHalf2(line_num));
                        }
                    } else {
                        for offset in 0..size as u64 {
                            state.mem_last_def.insert(addr + offset, DepNode::Line(line_num));
                        }
                    }
                }
            }

            state.deps[line_num] = line_deps;
        }

        state.line_count += 1;
        if line_num % 10000 == 0 && line_num > 0 {
            println!("已扫描 {} 行...", line_num);
        }
    }

    println!("扫描完成，共处理 {} 行", state.line_count);
    Ok(state)
}
