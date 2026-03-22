use std::collections::{BinaryHeap, HashSet};
use std::cmp::Ordering;
use crate::search_service::{SearchService, SearchConfig};
use crate::insn_analyzer::{InsnType, ParsedInsn};

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaintTarget {
    Reg(String, Option<(usize, usize)>), // reg_name, optional (offset, size)
    Mem(u64, usize), // addr, size
}

impl std::fmt::Display for TaintTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaintTarget::Reg(r, None) => write!(f, "Reg({})", r),
            TaintTarget::Reg(r, Some((off, sz))) => write!(f, "Reg({}[{}:{}])", r, off, off + sz),
            TaintTarget::Mem(addr, size) => write!(f, "Mem(0x{:x}, {})", addr, size),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, PartialOrd, Ord)]
pub struct SearchResultItem {
    pub line_num: usize,
    pub target: TaintTarget,
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
            TaintTarget::Reg(reg, _offset) => {
                let pattern = ParsedInsn::gen_reg_write_pattern(reg, "");
                let config = SearchConfig::new(pattern.pattern.clone()).with_regex(pattern.is_regex);
                
                // 检查当前行是否就是定义点
                if let Some(text) = self.service.get_line_text(current_line) {
                    if pattern.is_regex {
                        if let Ok(re) = regex::Regex::new(&config.pattern) {
                            if re.is_match(&text) {
                                return Some(current_line);
                            }
                        }
                    } else {
                        if text.contains(&config.pattern) {
                            return Some(current_line);
                        }
                    }
                }
                
                if let Some(res) = self.service.find_prev(current_line, config) {
                    return Some(res.line_number);
                }
            }
            TaintTarget::Mem(addr, size) => {
                let mut curr = current_line;
                // 先检查当前行
                if let Some(text) = self.service.get_line_text(curr) {
                    let parsed = ParsedInsn::parse(&text);
                    if let Ok((_src_regs, write_addr, write_size)) = parsed.get_store_info() {
                        if ParsedInsn::check_memory_overlap(*addr, *size, write_addr, write_size).is_some() {
                            return Some(curr);
                        }
                    }
                    
                    let mut st_idx = 0;
                    while let Some(pos) = text[st_idx..].find("st__") {
                        let start = st_idx + pos;
                        let end = text[start..].find(';').map(|i| start + i).unwrap_or(text.len());
                        let st_info = &text[start + 4..end];
                        let info_parts: Vec<&str> = st_info.split('_').collect();
                        if info_parts.len() >= 2 {
                            if let Ok(w_addr) = u64::from_str_radix(info_parts[0], 16) {
                                let w_size_str = info_parts[1].trim_matches(|c: char| !c.is_numeric());
                                if let Ok(w_size) = w_size_str.parse::<usize>() {
                                    if ParsedInsn::check_memory_overlap(*addr, *size, w_addr, w_size).is_some() {
                                        return Some(curr);
                                    }
                                }
                            }
                        }
                        st_idx = start + 4;
                    }
                }
                
                let config = SearchConfig::new("st__".to_string()).with_regex(false);
                let mut attempts = 0;
                loop {
                    if curr == 0 { break; }
                    
                    if let Some(res) = self.service.find_prev(curr, config.clone()) {
                        let res_line = res.line_number;
                        
                        if let Some(text) = self.service.get_line_text(res_line) {
                            let parsed = ParsedInsn::parse(&text);
                            
                            if let Ok((_src_regs, write_addr, write_size)) = parsed.get_store_info() {
                                if ParsedInsn::check_memory_overlap(*addr, *size, write_addr, write_size).is_some() {
                                    return Some(res_line);
                                }
                            }
                            
                        let mut st_idx = 0;
                        while let Some(pos) = text[st_idx..].find("st__") {
                            let start = st_idx + pos;
                            let end = text[start..].find(';').map(|i| start + i).unwrap_or(text.len());
                            let st_info = &text[start + 4..end]; // skip "st__"
                            let info_parts: Vec<&str> = st_info.split('_').collect();
                            if info_parts.len() >= 2 {
                                if let Ok(w_addr) = u64::from_str_radix(info_parts[0], 16) {
                                    let w_size_str = info_parts[1].trim_matches(|c: char| !c.is_numeric());
                                    if let Ok(w_size) = w_size_str.parse::<usize>() {
                                        if ParsedInsn::check_memory_overlap(*addr, *size, w_addr, w_size).is_some() {
                                            return Some(res_line);
                                        }
                                    }
                                }
                            }
                            st_idx = start + 4;
                        }
                        }
                        
                        curr = res_line;
                        attempts += 1;
                        if attempts > 50000 { // 放大上限
                            println!("警告: 内存搜索尝试超过 50000 次，放弃搜索 0x{:x}[{}] (停在 {})", addr, size, curr);
                            break;
                        }
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
                    TaintTarget::Reg(_, _) => reg_hit = true,
                    TaintTarget::Mem(_, _) => mem_hit = true,
                }
            }
            
            // If memory was hit, it means this line is a Store that defined the memory.
            // We trace the sources of this Store (the registers being stored).
            if mem_hit {
                if let Ok((src_regs, write_addr, _size)) = parsed.get_store_info() {
                    for t in &hit_targets {
                        if let TaintTarget::Mem(target_addr, target_size) = t {
                            if let Some((offset_in_write, overlap_size)) = ParsedInsn::check_memory_overlap(*target_addr, *target_size, write_addr, _size) {
                                let mut current_offset = 0;
                                for r in &src_regs {
                                    let r_size = ParsedInsn::get_reg_size(r);
                                    if current_offset + r_size > offset_in_write && current_offset < offset_in_write + overlap_size {
                                        let reg_offset = if offset_in_write > current_offset { offset_in_write - current_offset } else { 0 };
                                        let reg_end = (current_offset + r_size).min(offset_in_write + overlap_size);
                                        let overlap_in_reg_size = reg_end - current_offset.max(offset_in_write);
                                        
                                        if !ParsedInsn::is_zero_register(r) {
                                            new_targets.push(TaintTarget::Reg(r.clone(), Some((reg_offset, overlap_in_reg_size))));
                                        }
                                    }
                                    current_offset += r_size;
                                }
                            }
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
                                if let TaintTarget::Reg(hit_reg, reg_offset_info) = t {
                                    if let Some(idx) = dst_regs.iter().position(|r| r == hit_reg) {
                                        let r_size = ParsedInsn::get_reg_size(hit_reg);
                                        let mut adjusted_addr = base_addr + (idx * r_size) as u64;
                                        let mut mem_size = r_size;
                                        
                                        if let Some((offset, size)) = reg_offset_info {
                                            adjusted_addr += *offset as u64;
                                            mem_size = *size;
                                        }
                                        
                                        new_targets.push(TaintTarget::Mem(adjusted_addr, mem_size));
                                    }
                                }
                            }
                        }
                    }
                    InsnType::Arith | InsnType::Other | InsnType::Store => {
                        for (r, _val) in &parsed.read_regs {
                            if !ParsedInsn::is_zero_register(r) {
                                new_targets.push(TaintTarget::Reg(r.clone(), None));
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
