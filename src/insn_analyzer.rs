// insn_analyzer.rs
// 指令分析模块：解析指令格式，生成搜索pattern，处理污点传播逻辑
use anyhow::{Result, anyhow};
use egui::debug_text::print;

// 常见的内存访问大小（字节）
const COMMON_MEM_SIZES: &[usize] = &[16, 8, 4, 2, 1];

/// 指令类型（简化版本）
#[derive(Debug, Clone, PartialEq)]
pub enum InsnType {
    Load,       // 内存加载指令 (mem2reg): ldr, ldp, ldur等
    Store,      // 内存存储指令 (reg2mem): str, stp, stur等
    Arith,      // 算术/分支/寄存器类型变化（q0 = x1,x2）
    Other,      // 数据仅在reg间传递 的所有其他指令
}

/// 解析后的指令结构体 - 一次解析，多次访问
#[derive(Debug, Clone)]
pub struct ParsedInsn {
    pub raw_text: String,

    // 原始CSV字段
    pub full_addr: String,
    pub offset: String,
    pub asm: String,
    pub opcode: String,
    pub operands: String,
    pub read_regs_raw: String,
    pub mem_detail: String,
    pub mem_info: String,
    pub write_regs_raw: String,
    pub strvis: String,
    // 指令的分类
    pub insn_type: InsnType,
    // 寄存器信息
    pub read_regs: Vec<(String, String)>,   // (寄存器名, 值)
    pub write_regs: Vec<(String, String)>,  // (寄存器名, 值)
    // 内存访问信息
    pub mem_addr: Option<u64>,
    pub mem_size: Option<usize>,
    pub mem_access_type: u8, // 0: 无, 1: load, 2: store
    // 可能的内存地址

}

impl ParsedInsn {
    /// 从文本行解析指令（一次性解析所有信息）
    pub fn parse(line_text: &str) -> Self {
        let parts: Vec<&str> = line_text.split(';').collect();
        // 从csv中提取原始字段
        let [full_addr, offset, asm, opcode, operands, read_regs_raw, mem_detail, mem_info, write_regs_raw, strvis] =
            std::array::from_fn(|i| parts.get(i).unwrap_or(&"").to_string());
        // 提取mem信息
        let (mem_addr, mem_size, mem_access_type) = Self::extract_mem_info(&mem_info);
        // 获取r/w 的寄存器
        let read_regs = Self::extract_reg_values(&read_regs_raw);
        let write_regs = Self::extract_reg_values(&write_regs_raw);
        // 判断此条指令的类型
        let insn_type = Self::identify_type(&opcode,&read_regs,&write_regs);
        // 通过insn_type生成搜索下一步的 addr/reg 内存/寄存器 正则表达式；用到shadow_mem_reg

        // 结构体赋值
        Self {
            raw_text: line_text.to_string(),
            full_addr,
            offset,
            asm,
            opcode,
            operands,
            read_regs_raw,
            mem_detail,
            mem_info,
            write_regs_raw,
            strvis,
            insn_type,
            read_regs,
            write_regs,
            mem_addr,
            mem_size,
            mem_access_type,
        }
    }
    
    /// 识别指令类型（内部方法）
    fn identify_type(
        insn_name: &str,
        read_regs: &[(String, String)],
        write_regs: &[(String, String)]
    ) -> InsnType {
        let insn = insn_name.to_lowercase();

        match insn.as_str() {
            s if s.starts_with("ld") => InsnType::Load,
            s if s.starts_with("st") => InsnType::Store,
            // 如果rr和rw都为1且相等，那么认为是只数据转移不分支，即使数据变化
            _ if read_regs.len() == 1 && write_regs.len() == 1 => InsnType::Other,
            // 这种情况是产生了数据分支
            _ => InsnType::Arith,
        }
    }


    /// 从寄存器字段中提取值
    /// "“ -> []
    /// rr__x9=0x6f1c4518cf -> ((x9,0x6f1c4518cf))
    /// rr__w22=0x1_w23=0x2 -> ((w22,0x1),(w23,0x2))
    fn extract_reg_values(field: &str) -> Vec<(String, String)> {
        if field.is_empty() { return vec![]; }
        field[4..]
            .split('_')
            .filter_map(|pair| pair.split_once('='))
            .map(|(reg, val)| (reg.to_string(), val.to_string()))
            .collect()
    }
    
    /// 提取内存访问信息 ld__6cf01586a0_4
    fn extract_mem_info(mem_info: &str) -> (Option<u64>, Option<usize>, u8) {
        let (mem_access_type, mem_seg) = match () {
            _ if mem_info.starts_with("ld__") => (1, &mem_info[4..]),
            _ if mem_info.starts_with("st__") => (2, &mem_info[4..]),
            _ => return (None, None, 0),
        };

        let mut parts = mem_seg.split('_');
        let mem_addr = u64::from_str_radix(parts.next().unwrap(), 16).ok();
        let mem_size = parts.next().unwrap().parse().ok();

        (mem_addr, mem_size, mem_access_type)
    }

    /// 获取读取的寄存器列表（仅名称）
    pub fn get_read_reg_names(&self) -> Vec<String> {
        self.read_regs.iter().map(|(reg, _)| reg.clone()).collect()
    }
    
    /// 获取写入的寄存器列表（仅名称）
    pub fn get_write_reg_names(&self) -> Vec<String> {
        self.write_regs.iter().map(|(reg, _)| reg.clone()).collect()
    }
    
    /// 获取Load指令信息 (目标寄存器列表, 内存地址, 访问大小)
    pub fn get_load_info(&self) -> Result<(Vec<String>, u64, usize)> {
        let addr = self.mem_addr.ok_or_else(|| anyhow!("No memory address"))?;
        let size = self.mem_size.ok_or_else(|| anyhow!("No memory size"))?;
        
        Ok((self.get_write_reg_names(), addr, size))
    }
    
    /// 获取Store指令信息 (源寄存器列表, 内存地址, 访问大小)
    pub fn get_store_info(&self) -> Result<(Vec<String>, u64, usize)> {
        let addr = self.mem_addr.ok_or_else(|| anyhow!("No memory address"))?;
        let size = self.mem_size.ok_or_else(|| anyhow!("No memory size"))?;
        
        Ok((self.get_read_reg_names(), addr, size))
    }
    
    /// 生成内存读取的搜索pattern列表（按优先级排序）TODO 重写这里的逻辑
    pub fn gen_mem_read_patterns(addr: u64, size: usize) -> Vec<SearchPattern> {
        let mut patterns = Vec::new();
        
        // 优先级1: 精确匹配地址（任意大小）
        patterns.push(SearchPattern {
            pattern: format!("{}{:x}_", "st__", addr),
            is_regex: false,
            description: format!("精确匹配: 写入地址 0x{:x}", addr),
        });
        
        // 优先级2-N: 向前查找可能覆盖该地址的写入
        // 对于常见的写入大小 (1, 2, 4, 8, 16 字节)，计算可能的起始地址
        for &write_size in COMMON_MEM_SIZES {
            // 计算可能的写入起始地址范围
            // 如果 write_addr + write_size > addr，则可能覆盖
            // 即 write_addr > addr - write_size
            if addr >= write_size as u64 {
                let _min_write_addr = addr - write_size as u64 + 1;
                
                // 生成该范围内的对齐地址
                // ARM64 通常按 1, 2, 4, 8, 16 字节对齐
                for offset in 1..=write_size {
                    let candidate_addr = addr - offset as u64;
                    
                    // 只添加对齐的地址（提高效率）
                    if Self::is_aligned(candidate_addr, write_size) {
                        patterns.push(SearchPattern {
                            pattern: format!("{}{:x}_{}", "st__", candidate_addr, write_size),
                            is_regex: false,
                            description: format!(
                                "重叠匹配: 写入 0x{:x} ({} 字节) 可能覆盖 0x{:x}",
                                candidate_addr, write_size, addr
                            ),
                        });
                    }
                }
            }
        }
        
        patterns
    }
    
    /// 检查地址是否按指定大小对齐
    fn is_aligned(addr: u64, size: usize) -> bool {
        match size {
            1 => true,  // 1字节总是对齐
            2 => addr % 2 == 0,
            4 => addr % 4 == 0,
            8 => addr % 8 == 0,
            16 => addr % 16 == 0,
            _ => addr % size as u64 == 0,
        }
    }

    /// 检查内存访问是否重叠
    pub fn check_memory_overlap(
        read_addr: u64,
        read_size: usize,
        write_addr: u64,
        write_size: usize
    ) -> Option<(usize, usize)> {
        let read_end = read_addr + read_size as u64;
        let write_end = write_addr + write_size as u64;

        // 检查是否有重叠
        if write_addr < read_end && read_addr < write_end {
            // 计算重叠区域
            let overlap_start = read_addr.max(write_addr);
            let overlap_end = read_end.min(write_end);

            // 计算在写入范围内的偏移
            let offset_in_write = (overlap_start - write_addr) as usize;
            let overlap_size = (overlap_end - overlap_start) as usize;

            Some((offset_in_write, overlap_size))
        } else {
            None
        }
    }

    /// 生成寄存器写入的搜索pattern
    pub fn gen_reg_write_pattern(reg: &str, value: &str) -> SearchPattern {
        // 去掉寄存器前缀 (x0 -> 0, w8 -> 8)
        let reg_num = reg.trim_start_matches(|c: char| !c.is_numeric());
        let pattern = format!("rw_.*{}={}", reg_num, value);
        
        SearchPattern {
            pattern: pattern.clone(),
            is_regex: true,
            description: format!("查找写入寄存器 {} 值为 {} 的指令", reg, value),
        }
    }

    /// 生成寄存器读取的搜索pattern
    pub fn gen_reg_read_pattern(reg: &str, value: &str) -> SearchPattern {
        let reg_num = reg.trim_start_matches(|c: char| !c.is_numeric());
        let pattern = format!("rw_.*{}={}", reg_num, value);
        
        SearchPattern {
            pattern: pattern.clone(),
            is_regex: false,
            description: format!("查找寄存器 {} 被写入值 {} 的位置", reg, value),
        }
    }

    /// 生成算术运算的搜索pattern
    pub fn gen_arith_patterns(src_regs: &[(String, String)]) -> Vec<SearchPattern> {
        src_regs.iter()
            .map(|(reg, val)| {
                let reg_num = reg.trim_start_matches(|c: char| !c.is_numeric());
                // 搜索写入该寄存器且值匹配的指令
                let pattern = format!("rw_.*{}={}", reg_num, val);
                
                SearchPattern {
                    pattern: pattern.clone(),
                    is_regex: true,
                    description: format!("查找写入寄存器 {} 值为 {} 的指令", reg, val),
                }
            })
            .collect()
    }

    /// 判断是否是常量/立即数
    pub fn is_constant_value(value: &str) -> bool { value.starts_with('#') || value.contains("zr") }
    
    /// 判断寄存器是否是零寄存器（常量）
    pub fn is_zero_register(reg: &str) -> bool {
        reg == "wzr" || reg == "xzr" || reg == "zr"
    }

    /// 获取寄存器大小（字节）
    pub fn get_reg_size(reg: &str) -> usize {
        if reg.is_empty() {
            return 8;
        }
        
        match reg.chars().next() {
            Some('x') => 8,   // 64-bit
            Some('w') => 4,   // 32-bit
            Some('q') => 16,  // 128-bit SIMD
            Some('d') => 8,   // 64-bit SIMD
            Some('s') => 4,   // 32-bit SIMD
            Some('h') => 2,   // 16-bit
            Some('b') => 1,   // 8-bit
            _ => 8,
        }
    }

    /// 计算调整后的内存地址和大小
    pub fn calculate_adjusted_address(
        dst_regs: &[String],
        addr: u64,
        size: usize,
        target: &str,
        current_byte_range: &Option<(String, usize, usize)>,
    ) -> (u64, usize) {
        if dst_regs.is_empty() {
            return (addr, size);
        }

        if let Some((target_reg, byte_offset, byte_size)) = current_byte_range {
            if let Some(reg_index) = dst_regs.iter().position(|r| r == target_reg) {
                let reg_size = Self::get_reg_size(target_reg);
                let mem_offset = reg_index * reg_size;
                let new_addr = addr + mem_offset as u64 + *byte_offset as u64;
                println!("  [字节追踪] 寄存器 {} 在位置 {}, 调整搜索: 0x{:x}[{}] -> 0x{:x}[{}]",
                         target_reg, reg_index, addr, size, new_addr, *byte_size);
                return (new_addr, *byte_size);
            }
        }

        if dst_regs.len() > 1 {
            if let Some(reg_index) = dst_regs.iter().position(|r| r == target) {
                let reg_size = Self::get_reg_size(target);
                let mem_offset = reg_index * reg_size;
                let new_addr = addr + mem_offset as u64;
                println!("  [多寄存器] 寄存器 {} 在位置 {}, 调整搜索: 0x{:x}[{}] -> 0x{:x}[{}]",
                         target, reg_index, addr, size, new_addr, reg_size);
                return (new_addr, reg_size);
            }
        }

        (addr, size)
    }
}

/// 搜索模式
#[derive(Debug, Clone)]
pub struct SearchPattern {
    pub pattern: String,
    pub is_regex: bool,
    pub description: String,
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_load_insn() {
        let line = "6d2d76e78c;12b78c;b94c03e8;ldr;w8, [sp, #0xc00];;mr__6cf0157aa0_#0xc00;ld__6cf01586a0_4;rw__w8=0x1;";
        
        let parsed = ParsedInsn::parse(line);
        let (dst_regs, addr, size) = parsed.get_load_info().unwrap();
        
        assert_eq!(addr, 0x6cf01586a0);
        assert_eq!(size, 4);
        assert_eq!(dst_regs, vec!["w8"]);
    }

    #[test]
    fn test_parse_store_insn() {
        let line = "6d2d694ed4;51ed4;3d800260;str;q0, [x19];rr__q0=0x0x0100000001000000e061f2cb6c000000;mw__6cf01586a0;st__6cf01586a0_16;;";
        
        let parsed = ParsedInsn::parse(line);
        let (src_regs, addr, size) = parsed.get_store_info().unwrap();
        
        assert_eq!(addr, 0x6cf01586a0);
        assert_eq!(size, 16);
        assert_eq!(src_regs, vec!["q0"]);
    }

    #[test]
    fn test_identify_insn_type() {
        assert_eq!(
            ParsedInsn::parse(";;;ldr;w8, [sp]").insn_type,
            InsnType::Load
        );
        
        assert_eq!(
            ParsedInsn::parse(";;;str;q0, [x19]").insn_type,
            InsnType::Store
        );
        
        assert_eq!(
            ParsedInsn::parse(";;;add;w22, w22, #1").insn_type,
            InsnType::Arith
        );
        
        assert_eq!(
            ParsedInsn::parse(";;;mov;x0, x1").insn_type,
            InsnType::Arith
        );
        
        assert_eq!(
            ParsedInsn::parse(";;;cbz;x0, #0x1234").insn_type,
            InsnType::Arith
        );
    }

    #[test]
    fn test_gen_patterns() {
        // 测试启发式搜索 patterns
        // 场景: ldr w8, [0x6cf01586a0] (读4字节)
        let patterns = ParsedInsn::gen_mem_read_patterns(0x6cf01586a0, 4);
        
        println!("生成的搜索 patterns:");
        for (i, p) in patterns.iter().enumerate() {
            println!("  [{}] {}: {}", i, p.description, p.pattern);
        }
        
        // 优先级1: 精确匹配地址
        assert_eq!(patterns[0].pattern, "st__6cf01586a0_");
        assert!(!patterns[0].is_regex);
        
        // 应该包含重叠匹配的地址
        // 例如: st__6cf0158698_8 (0x98 + 8 = 0xa0)
        let has_0x98_8 = patterns.iter().any(|p| p.pattern == "st__6cf0158698_8");
        assert!(has_0x98_8, "应该包含 st__6cf0158698_8");
        
        // 例如: st__6cf0158690_16 (0x90 + 16 = 0xa0)
        let has_0x90_16 = patterns.iter().any(|p| p.pattern == "st__6cf0158690_16");
        assert!(has_0x90_16, "应该包含 st__6cf0158690_16");
        
        // 测试寄存器 pattern
        let pattern = ParsedInsn::gen_reg_write_pattern("w8", "0x1");
        assert_eq!(pattern.pattern, "rw_.*8=0x1");
        assert!(pattern.is_regex);
        
        // 测试算术运算 pattern
        let reg_values = vec![("w22".to_string(), "0x0".to_string())];
        let patterns = ParsedInsn::gen_arith_patterns(&reg_values);
        assert_eq!(patterns[0].pattern, "rw_.*22=0x0");
        assert!(patterns[0].is_regex);
    }
    
    #[test]
    fn test_alignment() {
        assert!(ParsedInsn::is_aligned(0x100, 1));
        assert!(ParsedInsn::is_aligned(0x100, 2));
        assert!(ParsedInsn::is_aligned(0x100, 4));
        assert!(ParsedInsn::is_aligned(0x100, 8));
        assert!(ParsedInsn::is_aligned(0x100, 16));
        
        assert!(!ParsedInsn::is_aligned(0x101, 2));
        assert!(!ParsedInsn::is_aligned(0x102, 4));
        assert!(!ParsedInsn::is_aligned(0x104, 8));
        assert!(!ParsedInsn::is_aligned(0x108, 16));
    }

    #[test]
    fn test_memory_overlap() {
        // 场景1: ldr w8, [0x6cf01586a0] (4字节) vs str q0, [0x6cf01586a0] (16字节)
        // 完全覆盖
        let result = ParsedInsn::check_memory_overlap(
            0x6cf01586a0, 4,  // read
            0x6cf01586a0, 16  // write
        );
        assert_eq!(result, Some((0, 4))); // write[0:4] -> read[0:4]

        // 场景2: ldr x21, [0x6cf01586a8] (8字节) vs str q0, [0x6cf01586a0] (16字节)
        // 部分覆盖
        let result = ParsedInsn::check_memory_overlap(
            0x6cf01586a8, 8,  // read: 0xa8-0xaf
            0x6cf01586a0, 16  // write: 0xa0-0xaf
        );
        assert_eq!(result, Some((8, 8))); // write[8:16] -> read[0:8]

        // 场景3: ldr w8, [0x6cf01586b0] (4字节) vs str q0, [0x6cf01586a0] (16字节)
        // 不重叠
        let result = ParsedInsn::check_memory_overlap(
            0x6cf01586b0, 4,  // read: 0xb0-0xb3
            0x6cf01586a0, 16  // write: 0xa0-0xaf
        );
        assert_eq!(result, None);

        // 场景4: ldr x0, [0x6cf01586a4] (8字节) vs str q0, [0x6cf01586a0] (16字节)
        // 部分覆盖（跨中间）
        let result = ParsedInsn::check_memory_overlap(
            0x6cf01586a4, 8,  // read: 0xa4-0xab
            0x6cf01586a0, 16  // write: 0xa0-0xaf
        );
        assert_eq!(result, Some((4, 8))); // write[4:12] -> read[0:8]
    }
}
