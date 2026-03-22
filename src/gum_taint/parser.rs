use crate::insn_analyzer::ParsedInsn;
use crate::gum_taint::category::{InsnCategory, classify_mnemonic, parse_reg_name, REG_INVALID};

#[derive(Debug, Clone)]
pub struct TraceLine {
    pub line_number: usize,
    pub category: InsnCategory,
    
    pub dst_regs: Vec<usize>,
    pub src_regs: Vec<usize>,
    
    pub has_mem_read: bool,
    pub mem_read_addr: u64,
    pub mem_read_size: u64,
    pub has_mem_read2: bool,
    pub mem_read_addr2: u64,
    pub mem_read_size2: u64,
    
    pub has_mem_write: bool,
    pub mem_write_addr: u64,
    pub mem_write_size: u64,
    pub has_mem_write2: bool,
    pub mem_write_addr2: u64,
    pub mem_write_size2: u64,
    
    pub sets_flags: bool,
    
    pub raw_text: String,
}

impl TraceLine {
    pub fn parse(line_number: usize, text: &str) -> Option<Self> {
        if text.trim().is_empty() { return None; }
        
        let parsed = ParsedInsn::parse(text);
        let category = classify_mnemonic(&parsed.opcode);
        
        let mut dst_regs = Vec::new();
        for (reg, _) in &parsed.write_regs {
            let id = parse_reg_name(reg);
            if id != REG_INVALID {
                dst_regs.push(id);
            }
        }
        
        let mut src_regs = Vec::new();
        for (reg, _) in &parsed.read_regs {
            let id = parse_reg_name(reg);
            if id != REG_INVALID {
                src_regs.push(id);
            }
        }
        
        let mut has_mem_read = false;
        let mut mem_read_addr = 0;
        let mut mem_read_size = 0;
        let mut has_mem_write = false;
        let mut mem_write_addr = 0;
        let mut mem_write_size = 0;
        
        if parsed.mem_access_type == 1 {
            has_mem_read = true;
            mem_read_addr = parsed.mem_addr.unwrap_or(0);
            mem_read_size = if let Some((reg, _)) = parsed.write_regs.first() {
                ParsedInsn::get_reg_size(reg) as u64
            } else {
                8
            };
        } else if parsed.mem_access_type == 2 {
            has_mem_write = true;
            mem_write_addr = parsed.mem_addr.unwrap_or(0);
            mem_write_size = if let Some((reg, _)) = parsed.read_regs.first() {
                ParsedInsn::get_reg_size(reg) as u64
            } else {
                8
            };
        }
        
        // 双读/写检测
        let mut has_mem_read2 = false;
        let mut mem_read_addr2 = 0;
        let mut mem_read_size2 = 0;
        let mut has_mem_write2 = false;
        let mut mem_write_addr2 = 0;
        let mut mem_write_size2 = 0;
        
        let mnem = parsed.opcode.to_ascii_lowercase();
        let is_stp = mnem == "stp";
        let is_ldp = mnem == "ldp" || mnem == "ldpsw" || mnem == "ldxp" || mnem == "ldaxp" || mnem == "ldnp";
        
        if has_mem_write && is_stp {
            // 需要判断寄存器大小，默认按 8 字节处理
            let mut reg_size = 8;
            if let Some((reg, _)) = parsed.read_regs.first() {
                reg_size = ParsedInsn::get_reg_size(reg) as u64;
            }
            has_mem_write2 = true;
            mem_write_addr2 = mem_write_addr + reg_size;
            mem_write_size = reg_size;
            mem_write_size2 = reg_size;
        }
        
        if has_mem_read && is_ldp {
            let mut reg_size = 8;
            if let Some((reg, _)) = parsed.write_regs.first() {
                reg_size = ParsedInsn::get_reg_size(reg) as u64;
            }
            has_mem_read2 = true;
            mem_read_addr2 = mem_read_addr + reg_size;
            mem_read_size = reg_size;
            mem_read_size2 = reg_size;
        }
        
        let mut sets_flags = false;
        if category == InsnCategory::Compare {
            sets_flags = true;
        } else if mnem.ends_with('s') {
            if ["adds", "subs", "ands", "bics", "adcs", "sbcs", "negs", "ngcs"].contains(&mnem.as_str()) {
                sets_flags = true;
            }
        }
        
        Some(Self {
            line_number,
            category,
            dst_regs,
            src_regs,
            has_mem_read,
            mem_read_addr,
            mem_read_size,
            has_mem_read2,
            mem_read_addr2,
            mem_read_size2,
            has_mem_write,
            mem_write_addr,
            mem_write_size,
            has_mem_write2,
            mem_write_addr2,
            mem_write_size2,
            sets_flags,
            raw_text: text.to_string(),
        })
    }
}
