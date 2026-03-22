use std::collections::{BinaryHeap, HashSet};
use std::cmp::Ordering;
use crate::search_service::{SearchService, SearchConfig};
use crate::insn_analyzer::{InsnType, ParsedInsn};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum TaintTarget {
    Reg(String),
    Mem(u64, usize), // addr, size
}

impl std::fmt::Display for TaintTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaintTarget::Reg(r) => write!(f, "Reg({})", r),
            TaintTarget::Mem(addr, size) => write!(f, "Mem(0x{:x}, {})", addr, size),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct SearchResultItem {
    line_num: usize,
    target: TaintTarget,
}

// Custom ordering for Max-Heap based on line_num
impl Ord for SearchResultItem {
    fn cmp(&self, other: &Self) -> Ordering {
        self.line_num.cmp(&other.line_num)
    }
}

impl PartialOrd for SearchResultItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug)]
pub struct TaintStep {
    pub line_num: usize,
    pub instruction: String,
    pub hit_targets: Vec<TaintTarget>,
    pub new_targets: Vec<TaintTarget>,
}

pub struct TaintBySearchEngine<'a> {
    service: &'a mut SearchService,
    max_steps: usize,
}

impl<'a> TaintBySearchEngine<'a> {
    pub fn new(service: &'a mut SearchService, max_steps: usize) -> Self {
        Self { service, max_steps }
    }
    
    fn find_def(&mut self, current_line: usize, target: &TaintTarget) -> Option<usize> {
        match target {
            TaintTarget::Reg(reg) => {
                let pattern = ParsedInsn::gen_reg_write_pattern(reg, "");
                let config = SearchConfig::new(pattern.pattern).with_regex(pattern.is_regex);
                if let Some(res) = self.service.find_prev(current_line, config) {
                    return Some(res.line_number);
                }
            }
            TaintTarget::Mem(addr, size) => {
                let mut curr = current_line;
                let config = SearchConfig::new("st__".to_string()).with_regex(false);
                loop {
                    if let Some(res) = self.service.find_prev(curr, config.clone()) {
                        if let Some(text) = self.service.get_line_text(res.line_number) {
                            let parsed = ParsedInsn::parse(&text);
                            if let Ok((_src_regs, write_addr, write_size)) = parsed.get_store_info() {
                                if ParsedInsn::check_memory_overlap(*addr, *size, write_addr, write_size).is_some() {
                                    return Some(res.line_number);
                                }
                            }
                        }
                        curr = res.line_number;
                        if curr == 0 { break; }
                    } else {
                        break;
                    }
                }
            }
        }
        None
    }

    pub fn trace(&mut self, start_line: usize, initial_targets: Vec<TaintTarget>) -> Vec<TaintStep> {
        let mut pq = BinaryHeap::new();
        let mut chain = Vec::new();
        let mut visited = HashSet::new(); // prevent infinite loops if any
        
        for t in initial_targets {
            if let Some(def_line) = self.find_def(start_line, &t) {
                pq.push(SearchResultItem { line_num: def_line, target: t });
            }
        }
        
        let mut step_count = 0;
        while let Some(item) = pq.peek() {
            if step_count >= self.max_steps { break; }
            
            let l_max = item.line_num;
            
            // Collect all targets defined at this exact line
            let mut hit_targets = Vec::new();
            while let Some(top) = pq.peek() {
                if top.line_num == l_max {
                    let popped = pq.pop().unwrap();
                    if !hit_targets.contains(&popped.target) {
                        hit_targets.push(popped.target);
                    }
                } else {
                    break;
                }
            }
            
            if visited.contains(&l_max) {
                continue;
            }
            visited.insert(l_max);

            let text = self.service.get_line_text(l_max).unwrap_or_default();
            let parsed = ParsedInsn::parse(&text);
            let mut new_targets = Vec::new();
            
            let mut reg_hit = false;
            let mut mem_hit = false;
            
            for t in &hit_targets {
                match t {
                    TaintTarget::Reg(_) => reg_hit = true,
                    TaintTarget::Mem(_, _) => mem_hit = true,
                }
            }
            
            // If memory was hit, it means this line is a Store that defined the memory.
            // We trace the sources of this Store (the registers being stored).
            if mem_hit {
                if let Ok((src_regs, _addr, _size)) = parsed.get_store_info() {
                    for r in src_regs {
                        if !ParsedInsn::is_zero_register(&r) {
                            new_targets.push(TaintTarget::Reg(r));
                        }
                    }
                }
            }
            
            // If register was hit, we trace where the register got its value.
            if reg_hit {
                match parsed.insn_type {
                    InsnType::Load => {
                        if let Ok((dst_regs, base_addr, _base_size)) = parsed.get_load_info() {
                            for t in &hit_targets {
                                if let TaintTarget::Reg(hit_reg) = t {
                                    if let Some(idx) = dst_regs.iter().position(|r| r == hit_reg) {
                                        let r_size = ParsedInsn::get_reg_size(hit_reg);
                                        let adjusted_addr = base_addr + (idx * r_size) as u64;
                                        new_targets.push(TaintTarget::Mem(adjusted_addr, r_size));
                                    }
                                }
                            }
                        }
                    }
                    InsnType::Arith | InsnType::Other | InsnType::Store => {
                        for (r, _val) in &parsed.read_regs {
                            if !ParsedInsn::is_zero_register(r) {
                                new_targets.push(TaintTarget::Reg(r.clone()));
                            }
                        }
                    }
                }
            }
            
            // Deduplicate new_targets
            let mut unique_new = Vec::new();
            for nt in new_targets {
                if !unique_new.contains(&nt) {
                    unique_new.push(nt);
                }
            }
            
            // Push to PQ
            for nt in &unique_new {
                if let Some(def_line) = self.find_def(l_max, nt) {
                    pq.push(SearchResultItem { line_num: def_line, target: nt.clone() });
                }
            }
            
            chain.push(TaintStep {
                line_num: l_max,
                instruction: text,
                hit_targets,
                new_targets: unique_new,
            });
            
            step_count += 1;
        }
        
        chain
    }
}
