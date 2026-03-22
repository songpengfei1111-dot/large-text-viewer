use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;
use serde::{Deserialize, Serialize};
use anyhow::Result;
use crate::insn_analyzer::ParsedInsn;
use crate::search_service::SearchService;

/// 表示预处理后的指令骨架，只保留对 Taint 扫描有用的核心字段。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaintInsnSkeleton {
    // 是否为 LDP 指令
    pub is_ldp: bool,
    // 是否为 STP 指令
    pub is_stp: bool,
    // 内存访问类型：0: 无, 1: Load, 2: Store
    pub mem_access_type: u8,
    
    // 内存访问地址和大小 (若有)
    pub mem_addr: Option<u64>,
    pub mem_size: Option<usize>,
    
    // 归一化后的读取寄存器名
    pub read_regs: Vec<String>,
    // 归一化后的写入寄存器名
    pub write_regs: Vec<String>,
}

/// 将原始 CSV 扫描并解析，然后序列化到二进制缓存文件中
pub fn preprocess_to_cache(service: &mut SearchService, cache_path: &Path) -> Result<()> {
    println!("开始预处理并生成二进制缓存...");
    let total_lines = service.total_lines();
    
    let mut skeletons = Vec::with_capacity(total_lines);
    
    for line_num in 0..total_lines {
        if let Some(line_text) = service.get_line_text(line_num) {
            let parsed = ParsedInsn::parse(&line_text);
            
            let is_ldp = parsed.opcode.starts_with("ldp");
            let is_stp = parsed.opcode.starts_with("stp");
            
            // 提取并归一化读写寄存器
            let mut read_regs = Vec::new();
            for (reg_name, _) in &parsed.read_regs {
                if !ParsedInsn::is_zero_register(reg_name) {
                    read_regs.push(ParsedInsn::normalize_reg(reg_name));
                }
            }
            
            let mut write_regs = Vec::new();
            for (reg_name, _) in &parsed.write_regs {
                if !ParsedInsn::is_zero_register(reg_name) {
                    write_regs.push(ParsedInsn::normalize_reg(reg_name));
                }
            }
            
            skeletons.push(TaintInsnSkeleton {
                is_ldp,
                is_stp,
                mem_access_type: parsed.mem_access_type,
                mem_addr: parsed.mem_addr,
                mem_size: parsed.mem_size,
                read_regs,
                write_regs,
            });
        } else {
            // 如果读取失败，放入一个空骨架占位，保持行号对齐
            skeletons.push(TaintInsnSkeleton {
                is_ldp: false,
                is_stp: false,
                mem_access_type: 0,
                mem_addr: None,
                mem_size: None,
                read_regs: Vec::new(),
                write_regs: Vec::new(),
            });
        }
        
        if line_num % 10000 == 0 && line_num > 0 {
            println!("已预处理 {} 行...", line_num);
        }
    }
    
    // 使用 bincode 序列化并写入文件
    let file = File::create(cache_path)?;
    let mut writer = BufWriter::new(file);
    // 这里使用 bincode v1 还是 v3 的 API？ 根据刚才 cargo add bincode, 可能是 v3 或者 v1.
    // 稳妥起见，我们用 bincode 的经典 API 或者 serde 的 bincode 模块
    // 让我们使用 bincode 默认的序列化方法
    bincode::serialize_into(&mut writer, &skeletons)?;
    writer.flush()?;
    
    println!("预处理完成，缓存已保存至: {:?}", cache_path.display());
    Ok(())
}

/// 从二进制缓存文件中加载骨架数组
pub fn load_from_cache(cache_path: &Path) -> Result<Vec<TaintInsnSkeleton>> {
    println!("从缓存文件加载预处理数据: {:?}", cache_path.display());
    let file = File::open(cache_path)?;
    let reader = BufReader::new(file);
    let skeletons: Vec<TaintInsnSkeleton> = bincode::deserialize_from(reader)?;
    Ok(skeletons)
}
