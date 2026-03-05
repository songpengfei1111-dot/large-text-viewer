// insn_analyzer.rs
// 指令分析模块：解析指令格式，生成搜索pattern，处理污点传播逻辑
use anyhow::{Result, anyhow};

const SEP: &str = ";";

// 寄存器字段前缀常量
const PREFIX_REG_READ: &str = "rr__";
const PREFIX_REG_WRITE: &str = "rw__";
const PREFIX_MEM_LOAD: &str = "ld__";
const PREFIX_MEM_STORE: &str = "st__";

// 常见的内存访问大小（字节）
const COMMON_MEM_SIZES: &[usize] = &[16, 8, 4, 2, 1];

/// 指令类型
#[derive(Debug, Clone, PartialEq)]
pub enum InsnType {
    Load,       // 内存加载指令 (ldr, ldp, ldur等)
    Store,      // 内存存储指令 (str, stp, stur等)
    Move,       // 寄存器传递 (mov, mvn等)
    Arith,      // 算术运算 (add, sub, mul等)
    Logic,      // 逻辑运算 (and, orr, eor等)
    Branch,     // 分支指令 (cbz, cbnz, b等)
    Unknown,
}

/// 搜索模式
#[derive(Debug, Clone)]
pub struct SearchPattern {
    pub pattern: String,
    pub is_regex: bool,
    pub description: String,
}

/// 指令分析器
pub struct InsnAnalyzer;

impl InsnAnalyzer {
    /// 识别指令类型
    pub fn identify_insn_type(line_text: &str) -> InsnType {
        let parts: Vec<&str> = line_text.split(SEP).collect();
        
        // 指令名称通常在第4个字段
        if let Some(insn_name) = parts.get(3) {
            let insn = insn_name.trim().to_lowercase();
            
            const LOAD_PREFIXES: &[&str] = &["ldr", "ldp", "ldur", "ldar"];
            const STORE_PREFIXES: &[&str] = &["str", "stp", "stur", "stlr"];
            const MOVE_PREFIXES: &[&str] = &["mov", "mvn"];
            const ARITH_PREFIXES: &[&str] = &["add", "sub", "mul", "div", "neg", "adc", "sbc"];
            const LOGIC_PREFIXES: &[&str] = &["and", "orr", "eor", "bic", "orn", "eon"];
            const BRANCH_PREFIXES: &[&str] = &["cbz", "cbnz", "tbz", "tbnz"];
            
            if LOAD_PREFIXES.iter().any(|prefix| insn.starts_with(prefix)) {
                return InsnType::Load;
            }
            
            if STORE_PREFIXES.iter().any(|prefix| insn.starts_with(prefix)) {
                return InsnType::Store;
            }
            
            if MOVE_PREFIXES.iter().any(|prefix| insn.starts_with(prefix)) {
                return InsnType::Move;
            }
            
            if ARITH_PREFIXES.iter().any(|prefix| insn.starts_with(prefix)) {
                return InsnType::Arith;
            }
            
            if LOGIC_PREFIXES.iter().any(|prefix| insn.starts_with(prefix)) {
                return InsnType::Logic;
            }
            
            if BRANCH_PREFIXES.iter().any(|prefix| insn.starts_with(prefix)) || 
               insn == "b" || insn.starts_with("b.") {
                return InsnType::Branch;
            }
        }
        
        InsnType::Unknown
    }

    /// 通用的内存指令解析方法
    /// marker: "ld__" 或 "st__"
    /// 返回: (寄存器列表, 内存地址, 访问大小)
    fn parse_mem_insn(line_text: &str, marker: &str) -> Result<(Vec<String>, u64, usize)> {
        let parts: Vec<&str> = line_text.split(SEP).collect();
        
        // 查找标记来获取内存地址和大小
        let mem_info = parts.iter()
            .find(|p| p.starts_with(marker))
            .ok_or_else(|| anyhow!("No {} marker found", marker))?;
        
        // 解析 ld__6cf01586a0_4 或 st__6cf01586a0_16 -> addr, size
        let mem_parts: Vec<&str> = mem_info.split('_').collect();
        if mem_parts.len() < 3 {
            return Err(anyhow!("Invalid {} format", marker));
        }
        
        let addr_str = mem_parts[mem_parts.len() - 2];
        let size_str = mem_parts[mem_parts.len() - 1];
        
        let addr = u64::from_str_radix(addr_str, 16)
            .map_err(|_| anyhow!("Invalid address: {}", addr_str))?;
        let size = size_str.parse::<usize>()
            .map_err(|_| anyhow!("Invalid size: {}", size_str))?;
        
        // 根据标记类型提取对应的寄存器
        let regs = if marker == PREFIX_MEM_LOAD {
            Self::extract_write_regs(line_text)  // load 指令写入寄存器
        } else {
            Self::extract_read_regs(line_text)   // store 指令读取寄存器
        };
        
        Ok((regs, addr, size))
    }

    /// 解析加载指令 (ldr, ldp等)
    /// 返回: (目标寄存器列表, 内存地址, 访问大小)
    pub fn parse_load_insn(line_text: &str) -> Result<(Vec<String>, u64, usize)> {
        Self::parse_mem_insn(line_text, PREFIX_MEM_LOAD)
    }

    /// 解析存储指令 (str, stp等)
    /// 返回: (源寄存器列表, 内存地址, 访问大小)
    pub fn parse_store_insn(line_text: &str) -> Result<(Vec<String>, u64, usize)> {
        Self::parse_mem_insn(line_text, PREFIX_MEM_STORE)
    }

    /// 提取读取的寄存器 (rr__ 字段)
    pub fn extract_read_regs(line_text: &str) -> Vec<String> {
        Self::extract_reg_values(line_text, PREFIX_REG_READ)
            .into_iter()
            .map(|(reg, _)| reg)
            .collect()
    }

    /// 提取写入的寄存器 (rw__ 字段)
    pub fn extract_write_regs(line_text: &str) -> Vec<String> {
        Self::extract_reg_values(line_text, PREFIX_REG_WRITE)
            .into_iter()
            .map(|(reg, _)| reg)
            .collect()
    }

    /// 通用的寄存器提取方法（已废弃，使用 extract_reg_values 代替）
    #[deprecated(note = "Use extract_reg_values instead")]
    fn extract_regs_by_prefix(line_text: &str, prefix: &str) -> Vec<String> {
        Self::extract_reg_values(line_text, prefix)
            .into_iter()
            .map(|(reg, _)| reg)
            .collect()
    }

    /// 提取寄存器及其值 (rr__ 或 rw__ 字段)
    pub fn extract_reg_values(line_text: &str, prefix: &str) -> Vec<(String, String)> {
        line_text.split(SEP)
            .find(|p| p.starts_with(prefix))
            .and_then(|part| part.strip_prefix(prefix))
            .map(|s| {
                s.split('_')
                    .filter(|pair| pair.contains('='))
                    .filter_map(|pair| pair.split_once('='))
                    .map(|(reg, val)| (reg.to_string(), val.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 生成内存读取的搜索pattern列表（按优先级排序）
    /// 用于追踪: 哪条指令写入了这个内存地址
    /// 
    /// 策略：从最精确到最宽泛
    /// 1. 精确匹配：st__<addr>_* (任意大小写入该地址)
    /// 2. 重叠匹配：向前查找可能覆盖该地址的写入
    ///    - 如果写入地址 < 读取地址，且 写入地址 + 写入大小 > 读取地址，则重叠
    /// 
    /// 例如: ldr w8, [0x6cf01586a0] (读4字节，地址 0xa0)
    /// - 优先级1: st__6cf01586a0_* (精确匹配地址)
    /// - 优先级2: st__6cf0158698_8 (0x98+8=0xa0，刚好覆盖)
    /// - 优先级3: st__6cf0158690_16 (0x90+16=0xa0，刚好覆盖)
    /// - 优先级4: st__6cf0158698_16 (0x98+16>0xa0，覆盖)
    /// - 优先级5: st__6cf0158690_* (0x90开始的任意大小，可能覆盖)
    /// TODO 根据策略调整这里的优先级，双寄存器的情况下要转移
    pub fn gen_mem_read_patterns(addr: u64, size: usize) -> Vec<SearchPattern> {
        let mut patterns = Vec::new();
        
        // 优先级1: 精确匹配地址（任意大小）
        patterns.push(SearchPattern {
            pattern: format!("{}{:x}_", PREFIX_MEM_STORE, addr),
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
                let min_write_addr = addr - write_size as u64 + 1;
                
                // 生成该范围内的对齐地址
                // ARM64 通常按 1, 2, 4, 8, 16 字节对齐
                for offset in 1..write_size {
                    let candidate_addr = addr - offset as u64;
                    
                    // 只添加对齐的地址（提高效率）
                    if Self::is_aligned(candidate_addr, write_size) {
                        patterns.push(SearchPattern {
                            pattern: format!("{}{:x}_{}", PREFIX_MEM_STORE, candidate_addr, write_size),
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
    
    /// 生成单个内存读取的搜索pattern（向后兼容）
    pub fn gen_mem_read_pattern(addr: u64, size: usize) -> SearchPattern {
        SearchPattern {
            pattern: format!("{}{:x}_", PREFIX_MEM_STORE, addr),
            is_regex: false,
            description: format!("查找写入内存地址 0x{:x} 的指令", addr),
        }
    }

    /// 检查内存访问是否重叠
    /// read_addr: 读取的起始地址
    /// read_size: 读取的字节数
    /// write_addr: 写入的起始地址
    /// write_size: 写入的字节数
    /// 返回: (是否重叠, 写入偏移, 重叠字节数)
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
    /// 用于追踪: 哪条指令写入了这个寄存器
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
    /// 用于追踪: 这个寄存器的值从哪里来
    pub fn gen_reg_read_pattern(reg: &str, value: &str) -> SearchPattern {
        let pattern = format!("rw__{}={}", reg, value);
        
        SearchPattern {
            pattern: pattern.clone(),
            is_regex: false,
            description: format!("查找寄存器 {} 被写入值 {} 的位置", reg, value),
        }
    }

    /// 生成算术运算的搜索pattern
    /// 用于追踪: 源寄存器的值从哪里来
    /// 注意: 需要匹配读取的值（rr__），而不是写入的值（rw__）
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

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_load_insn() {
        let line = "6d2d76e78c;12b78c;b94c03e8;ldr;w8, [sp, #0xc00];;mr__6cf0157aa0_#0xc00;ld__6cf01586a0_4;rw__w8=0x1;";
        
        let (dst_regs, addr, size) = InsnAnalyzer::parse_load_insn(line).unwrap();
        
        assert_eq!(addr, 0x6cf01586a0);
        assert_eq!(size, 4);
        assert_eq!(dst_regs, vec!["w8"]);
    }

    #[test]
    fn test_parse_store_insn() {
        let line = "6d2d694ed4;51ed4;3d800260;str;q0, [x19];rr__q0=0x0x0100000001000000e061f2cb6c000000;mw__6cf01586a0;st__6cf01586a0_16;;";
        
        let (src_regs, addr, size) = InsnAnalyzer::parse_store_insn(line).unwrap();
        
        assert_eq!(addr, 0x6cf01586a0);
        assert_eq!(size, 16);
        assert_eq!(src_regs, vec!["q0"]);
    }

    #[test]
    fn test_identify_insn_type() {
        assert_eq!(
            InsnAnalyzer::identify_insn_type(";;;ldr;w8, [sp]"),
            InsnType::Load
        );
        
        assert_eq!(
            InsnAnalyzer::identify_insn_type(";;;str;q0, [x19]"),
            InsnType::Store
        );
        
        assert_eq!(
            InsnAnalyzer::identify_insn_type(";;;add;w22, w22, #1"),
            InsnType::Arith
        );
    }

    #[test]
    fn test_gen_patterns() {
        // 测试启发式搜索 patterns
        // 场景: ldr w8, [0x6cf01586a0] (读4字节)
        let patterns = InsnAnalyzer::gen_mem_read_patterns(0x6cf01586a0, 4);
        
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
        let pattern = InsnAnalyzer::gen_reg_write_pattern("w8", "0x1");
        assert_eq!(pattern.pattern, "rw_.*8=0x1");
        assert!(pattern.is_regex);
        
        // 测试算术运算 pattern
        let reg_values = vec![("w22".to_string(), "0x0".to_string())];
        let patterns = InsnAnalyzer::gen_arith_patterns(&reg_values);
        assert_eq!(patterns[0].pattern, "rw_.*22=0x0");
        assert!(patterns[0].is_regex);
    }
    
    #[test]
    fn test_alignment() {
        assert!(InsnAnalyzer::is_aligned(0x100, 1));
        assert!(InsnAnalyzer::is_aligned(0x100, 2));
        assert!(InsnAnalyzer::is_aligned(0x100, 4));
        assert!(InsnAnalyzer::is_aligned(0x100, 8));
        assert!(InsnAnalyzer::is_aligned(0x100, 16));
        
        assert!(!InsnAnalyzer::is_aligned(0x101, 2));
        assert!(!InsnAnalyzer::is_aligned(0x102, 4));
        assert!(!InsnAnalyzer::is_aligned(0x104, 8));
        assert!(!InsnAnalyzer::is_aligned(0x108, 16));
    }

    #[test]
    fn test_memory_overlap() {
        // 场景1: ldr w8, [0x6cf01586a0] (4字节) vs str q0, [0x6cf01586a0] (16字节)
        // 完全覆盖
        let result = InsnAnalyzer::check_memory_overlap(
            0x6cf01586a0, 4,  // read
            0x6cf01586a0, 16  // write
        );
        assert_eq!(result, Some((0, 4))); // write[0:4] -> read[0:4]

        // 场景2: ldr x21, [0x6cf01586a8] (8字节) vs str q0, [0x6cf01586a0] (16字节)
        // 部分覆盖
        let result = InsnAnalyzer::check_memory_overlap(
            0x6cf01586a8, 8,  // read: 0xa8-0xaf
            0x6cf01586a0, 16  // write: 0xa0-0xaf
        );
        assert_eq!(result, Some((8, 8))); // write[8:16] -> read[0:8]

        // 场景3: ldr w8, [0x6cf01586b0] (4字节) vs str q0, [0x6cf01586a0] (16字节)
        // 不重叠
        let result = InsnAnalyzer::check_memory_overlap(
            0x6cf01586b0, 4,  // read: 0xb0-0xb3
            0x6cf01586a0, 16  // write: 0xa0-0xaf
        );
        assert_eq!(result, None);

        // 场景4: ldr x0, [0x6cf01586a4] (8字节) vs str q0, [0x6cf01586a0] (16字节)
        // 部分覆盖（跨中间）
        let result = InsnAnalyzer::check_memory_overlap(
            0x6cf01586a4, 8,  // read: 0xa4-0xab
            0x6cf01586a0, 16  // write: 0xa0-0xaf
        );
        assert_eq!(result, Some((4, 8))); // write[4:12] -> read[0:8]
    }
}
