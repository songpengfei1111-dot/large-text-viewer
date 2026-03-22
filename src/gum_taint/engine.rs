use std::collections::HashSet;
use crate::gum_taint::category::{InsnCategory, REG_INVALID, REG_XZR, REG_NZCV};
use crate::gum_taint::parser::TraceLine;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackMode {
    Forward,
    Backward,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    AllTaintCleared,
    EndOfTrace,
    ScanLimitReached,
}

#[derive(Debug, Clone)]
pub struct TaintSource {
    pub reg: usize,
    pub mem_addr: u64,
    pub is_mem: bool,
}

impl TaintSource {
    pub fn from_reg(reg: usize) -> Self {
        Self { reg, mem_addr: 0, is_mem: false }
    }
    
    pub fn from_mem(addr: u64) -> Self {
        Self { reg: REG_INVALID, mem_addr: addr, is_mem: true }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MemBlock {
    pub addr: u64,
    pub size: u64,
}

pub struct ResultEntry {
    pub index: usize,
    pub reg_snapshot: [bool; 256],
    pub mem_snapshot: HashSet<MemBlock>,
    pub raw_text: String,
}

pub struct TaintEngine {
    mode: TrackMode,
    source: TaintSource,
    pub stop_reason: StopReason,
    pub max_scan_distance: usize,
    
    reg_taint: [bool; 256],
    tainted_reg_count: usize,
    // 改变这里：由 HashSet<u64> 改为 HashSet<MemBlock>，或者维护一个重叠合并的区间集合
    // 为了简单，我们先保持 HashSet<MemBlock>
    tainted_mem: HashSet<MemBlock>,
    
    pub results: Vec<ResultEntry>,
}

impl TaintEngine {
    pub fn new(mode: TrackMode, source: TaintSource) -> Self {
        let mut engine = Self {
            mode,
            source: source.clone(),
            stop_reason: StopReason::EndOfTrace,
            max_scan_distance: 1000000,
            reg_taint: [false; 256],
            tainted_reg_count: 0,
            tainted_mem: HashSet::new(),
            results: Vec::new(),
        };
        
        if source.is_mem {
            // Default to tracking 8 bytes if size is unknown from source
            engine.tainted_mem.insert(MemBlock { addr: source.mem_addr, size: 8 });
        } else {
            engine.taint_reg(source.reg);
        }
        
        engine
    }
    
    #[inline]
    fn taint_reg(&mut self, id: usize) {
        if id == REG_INVALID || id == REG_XZR { return; }
        if !self.reg_taint[id] {
            self.reg_taint[id] = true;
            self.tainted_reg_count += 1;
        }
    }
    
    #[inline]
    fn untaint_reg(&mut self, id: usize) {
        if id == REG_INVALID || id == REG_XZR { return; }
        if self.reg_taint[id] {
            self.reg_taint[id] = false;
            self.tainted_reg_count -= 1;
        }
    }
    
    #[inline]
    fn is_reg_tainted(&self, id: usize) -> bool {
        if id == REG_INVALID || id == REG_XZR { return false; }
        self.reg_taint[id]
    }
    
    fn any_src_tainted(&self, line: &TraceLine) -> bool {
        for &r in &line.src_regs {
            if self.is_reg_tainted(r) { return true; }
        }
        if line.has_mem_read && self.check_mem_overlap(line.mem_read_addr, line.mem_read_size).is_some() { return true; }
        if line.has_mem_read2 && self.check_mem_overlap(line.mem_read_addr2, line.mem_read_size2).is_some() { return true; }
        false
    }
    
    fn check_mem_overlap(&self, target_addr: u64, target_size: u64) -> Option<MemBlock> {
        for taint_block in &self.tainted_mem {
            let taint_addr = taint_block.addr;
            let taint_end = taint_addr + taint_block.size;
            let target_end = target_addr + target_size;
            
            let max_start = if taint_addr > target_addr { taint_addr } else { target_addr };
            let min_end = if taint_end < target_end { taint_end } else { target_end };
            
            if max_start < min_end {
                return Some(taint_block.clone());
            }
        }
        None
    }
    
    fn any_dst_tainted(&self, line: &TraceLine) -> bool {
        for &r in &line.dst_regs {
            if self.is_reg_tainted(r) { return true; }
        }
        if line.has_mem_write && self.check_mem_overlap(line.mem_write_addr, line.mem_write_size).is_some() { return true; }
        if line.has_mem_write2 && self.check_mem_overlap(line.mem_write_addr2, line.mem_write_size2).is_some() { return true; }
        false
    }
    
    pub fn record(&mut self, line: &TraceLine) {
        self.results.push(ResultEntry {
            index: line.line_number,
            reg_snapshot: self.reg_taint,
            mem_snapshot: self.tainted_mem.clone(),
            raw_text: line.raw_text.clone(),
        });
    }
    
    pub fn process_line(&mut self, line: &TraceLine) -> bool {
        let mut involved = false;
        
        if self.mode == TrackMode::Forward {
            involved = self.any_src_tainted(line);
            
            if !involved && line.has_mem_write && self.check_mem_overlap(line.mem_write_addr, line.mem_write_size).is_some() { involved = true; }
            if !involved && line.has_mem_write2 && self.check_mem_overlap(line.mem_write_addr2, line.mem_write_size2).is_some() { involved = true; }
            
            if involved {
                self.propagate_forward(line);
            }
            
        } else {
            involved = self.any_dst_tainted(line);
            
            if !involved && line.sets_flags && self.is_reg_tainted(REG_NZCV) { involved = true; }
            
            if involved {
                self.propagate_backward(line);
            }
        }
        
        if involved {
            self.record(line);
        }
        
        // 我们只在寄存器和内存都为空时才停止
        if self.tainted_reg_count == 0 && self.tainted_mem.is_empty() {
            self.stop_reason = StopReason::AllTaintCleared;
            return false; // Stop
        }
        
        involved
    }
    
    fn propagate_forward(&mut self, line: &TraceLine) {
        match line.category {
            InsnCategory::ImmLoad => {
                for &r in &line.dst_regs { self.untaint_reg(r); }
            }
            InsnCategory::PartialModify => {}
            InsnCategory::DataMove | InsnCategory::Arithmetic | InsnCategory::Logic |
            InsnCategory::ShiftExt | InsnCategory::Bitfield | InsnCategory::CondSelect => {
                let src_t = self.any_src_tainted(line);
                for &r in &line.dst_regs {
                    if src_t { self.taint_reg(r); } else { self.untaint_reg(r); }
                }
                if line.sets_flags {
                    if src_t { self.taint_reg(REG_NZCV); } else { self.untaint_reg(REG_NZCV); }
                }
            }
            InsnCategory::Load => {
                if line.has_mem_read2 && line.dst_regs.len() >= 2 {
                    let mem_t1 = line.has_mem_read && self.check_mem_overlap(line.mem_read_addr, line.mem_read_size).is_some();
                    let mem_t2 = self.check_mem_overlap(line.mem_read_addr2, line.mem_read_size2).is_some();
                    if mem_t1 { self.taint_reg(line.dst_regs[0]); } else { self.untaint_reg(line.dst_regs[0]); }
                    if mem_t2 { self.taint_reg(line.dst_regs[1]); } else { self.untaint_reg(line.dst_regs[1]); }
                } else {
                    let mem_t = line.has_mem_read && self.check_mem_overlap(line.mem_read_addr, line.mem_read_size).is_some();
                    for &r in &line.dst_regs {
                        if mem_t { self.taint_reg(r); } else { self.untaint_reg(r); }
                    }
                }
            }
            InsnCategory::Store => {
                if line.has_mem_write {
                    if line.has_mem_write2 && line.src_regs.len() >= 2 {
                        let src_t0 = self.is_reg_tainted(line.src_regs[0]);
                        let src_t1 = self.is_reg_tainted(line.src_regs[1]);
                        
                        if src_t0 { self.tainted_mem.insert(MemBlock { addr: line.mem_write_addr, size: line.mem_write_size }); }
                        else if let Some(blk) = self.check_mem_overlap(line.mem_write_addr, line.mem_write_size) { self.tainted_mem.remove(&blk); }
                        
                        if src_t1 { self.tainted_mem.insert(MemBlock { addr: line.mem_write_addr2, size: line.mem_write_size2 }); }
                        else if let Some(blk) = self.check_mem_overlap(line.mem_write_addr2, line.mem_write_size2) { self.tainted_mem.remove(&blk); }
                    } else {
                        let src_t = !line.src_regs.is_empty() && self.is_reg_tainted(line.src_regs[0]);
                        if src_t { 
                            self.tainted_mem.insert(MemBlock { addr: line.mem_write_addr, size: line.mem_write_size }); 
                        } else if let Some(blk) = self.check_mem_overlap(line.mem_write_addr, line.mem_write_size) { 
                            self.tainted_mem.remove(&blk); 
                        }
                    }
                }
            }
            InsnCategory::Compare => {
                let src_t = self.any_src_tainted(line);
                if src_t { self.taint_reg(REG_NZCV); } else { self.untaint_reg(REG_NZCV); }
            }
            InsnCategory::Branch => {}
            InsnCategory::Other => {
                let src_t = self.any_src_tainted(line);
                for &r in &line.dst_regs {
                    if src_t { self.taint_reg(r); } else { self.untaint_reg(r); }
                }
                if line.has_mem_write {
                    if src_t { 
                        self.tainted_mem.insert(MemBlock { addr: line.mem_write_addr, size: line.mem_write_size }); 
                    } else if let Some(blk) = self.check_mem_overlap(line.mem_write_addr, line.mem_write_size) { 
                        self.tainted_mem.remove(&blk); 
                    }
                }
                if line.has_mem_write2 {
                    if src_t { 
                        self.tainted_mem.insert(MemBlock { addr: line.mem_write_addr2, size: line.mem_write_size2 }); 
                    } else if let Some(blk) = self.check_mem_overlap(line.mem_write_addr2, line.mem_write_size2) { 
                        self.tainted_mem.remove(&blk); 
                    }
                }
            }
        }
    }
    
    fn propagate_backward(&mut self, line: &TraceLine) {
        match line.category {
            InsnCategory::ImmLoad => {
                for &r in &line.dst_regs {
                    if self.is_reg_tainted(r) { self.untaint_reg(r); }
                }
            }
            InsnCategory::PartialModify => {}
            InsnCategory::DataMove | InsnCategory::Arithmetic | InsnCategory::Logic |
            InsnCategory::ShiftExt | InsnCategory::Bitfield | InsnCategory::CondSelect => {
                let dst_t = self.any_dst_tainted(line);
                let nzcv_t = line.sets_flags && self.is_reg_tainted(REG_NZCV);
                if dst_t || nzcv_t {
                    for &r in &line.dst_regs { self.untaint_reg(r); }
                    if nzcv_t { self.untaint_reg(REG_NZCV); }
                    for &r in &line.src_regs { self.taint_reg(r); }
                }
            }
            InsnCategory::Load => {
                if line.has_mem_read2 && line.dst_regs.len() >= 2 {
                    let t0 = self.is_reg_tainted(line.dst_regs[0]);
                    let t1 = self.is_reg_tainted(line.dst_regs[1]);
                    if t0 {
                        self.untaint_reg(line.dst_regs[0]);
                        if line.has_mem_read { self.tainted_mem.insert(MemBlock { addr: line.mem_read_addr, size: line.mem_read_size }); }
                    }
                    if t1 {
                        self.untaint_reg(line.dst_regs[1]);
                        if line.has_mem_read2 { self.tainted_mem.insert(MemBlock { addr: line.mem_read_addr2, size: line.mem_read_size2 }); }
                    }
                } else {
                    let mut dst_t = false;
                    for &r in &line.dst_regs {
                        if self.is_reg_tainted(r) {
                            dst_t = true;
                            self.untaint_reg(r);
                        }
                    }
                    if dst_t && line.has_mem_read {
                        self.tainted_mem.insert(MemBlock { addr: line.mem_read_addr, size: line.mem_read_size });
                    }
                }
            }
            InsnCategory::Store => {
                if line.has_mem_write {
                    if line.has_mem_write2 && line.src_regs.len() >= 2 {
                        let mut to_remove = Vec::new();
                        for blk in &self.tainted_mem {
                            let taint_addr = blk.addr;
                            let taint_end = taint_addr + blk.size;
                            let target_end1 = line.mem_write_addr + line.mem_write_size;
                            let target_end2 = line.mem_write_addr2 + line.mem_write_size2;
                            
                            let max_start1 = if taint_addr > line.mem_write_addr { taint_addr } else { line.mem_write_addr };
                            let min_end1 = if taint_end < target_end1 { taint_end } else { target_end1 };
                            
                            let max_start2 = if taint_addr > line.mem_write_addr2 { taint_addr } else { line.mem_write_addr2 };
                            let min_end2 = if taint_end < target_end2 { taint_end } else { target_end2 };
                            
                            if max_start1 < min_end1 {
                                to_remove.push((blk.clone(), 0));
                            } else if max_start2 < min_end2 {
                                to_remove.push((blk.clone(), 1));
                            }
                        }
                        
                        for (blk, reg_idx) in to_remove {
                            self.tainted_mem.remove(&blk);
                            self.taint_reg(line.src_regs[reg_idx]);
                        }
                        
                    } else {
                        let mut to_remove = Vec::new();
                        for blk in &self.tainted_mem {
                            let taint_addr = blk.addr;
                            let taint_end = taint_addr + blk.size;
                            let target_end = line.mem_write_addr + line.mem_write_size;
                            
                            let max_start = if taint_addr > line.mem_write_addr { taint_addr } else { line.mem_write_addr };
                            let min_end = if taint_end < target_end { taint_end } else { target_end };
                            
                            if max_start < min_end {
                                to_remove.push(blk.clone());
                            }
                        }
                        
                        let mut erased = false;
                        for blk in to_remove {
                            self.tainted_mem.remove(&blk);
                            erased = true;
                        }
                        
                        if erased && !line.src_regs.is_empty() {
                            self.taint_reg(line.src_regs[0]);
                        }
                    }
                }
            }
            InsnCategory::Compare => {
                if self.is_reg_tainted(REG_NZCV) {
                    self.untaint_reg(REG_NZCV);
                    for &r in &line.src_regs { self.taint_reg(r); }
                }
            }
            InsnCategory::Branch => {}
            InsnCategory::Other => {
                let mut dst_t = self.any_dst_tainted(line);
                if line.has_mem_write && self.check_mem_overlap(line.mem_write_addr, line.mem_write_size).is_some() { dst_t = true; }
                if line.has_mem_write2 && self.check_mem_overlap(line.mem_write_addr2, line.mem_write_size2).is_some() { dst_t = true; }
                if dst_t {
                    for &r in &line.dst_regs { self.untaint_reg(r); }
                    if line.has_mem_write { 
                        if let Some(a) = self.check_mem_overlap(line.mem_write_addr, line.mem_write_size) { self.tainted_mem.remove(&a); } 
                    }
                    if line.has_mem_write2 { 
                        if let Some(a) = self.check_mem_overlap(line.mem_write_addr2, line.mem_write_size2) { self.tainted_mem.remove(&a); } 
                    }
                    for &r in &line.src_regs { self.taint_reg(r); }
                    if line.has_mem_read {
                        self.tainted_mem.insert(MemBlock { addr: line.mem_read_addr, size: line.mem_read_size });
                    }
                    if line.has_mem_read2 {
                        self.tainted_mem.insert(MemBlock { addr: line.mem_read_addr2, size: line.mem_read_size2 });
                    }
                }
            }
        }
    }
}
