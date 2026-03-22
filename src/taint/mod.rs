// src/taint/mod.rs
pub mod scanner;
pub mod slicer;
pub mod preprocessor;

use large_text_core::file_reader::FileReader;
use crate::search_service::SearchService;
use std::path::PathBuf;

pub fn test_def_use() -> anyhow::Result<()> {
    let file_path = std::path::PathBuf::from("/Users/bytedance/RustroverProjects/logs/record_01.csv");
    // 假设缓存文件放在同一目录下，后缀改为 .taint_cache
    let cache_path = PathBuf::from("/Users/bytedance/RustroverProjects/logs/record_01.taint_cache");
    
    let reader = FileReader::new(file_path, encoding_rs::UTF_8)?;
    let mut service = SearchService::new(reader);

    println!("=== 开始构建 Def-Use 依赖图 ===");
    let state = scanner::scan_pass(&mut service, Some(&cache_path))?;
    
    // 验证行号: 9028 (CSV中 0-based 索引为 9027)
    // 目标行: 6d2d76e788;12b788;f94607f4;ldr;x20, [sp, #0xc08];;mr__6cf0157aa0_#0xc08;ld__6cf01586a8_8;rw__x20=0x6ccbf261e0;
    let mut target_line: usize = 9027; 
    let target_reg = "x20"; 
    
    // 修正查找目标的逻辑，防止行号偏移
    for i in target_line.saturating_sub(5_usize)..target_line+5 {
        if let Some(text) = service.get_line_text(i) {
            if text.contains("ldr;x20, [sp, #0xc08]") && text.contains("6d2d76e788") {
                target_line = i;
                break;
            }
        }
    }
    
    let mut start_line = None;
    if let Some(text) = service.get_line_text(target_line) {
        println!("目标行内容: {}", text);
        let parsed = crate::insn_analyzer::ParsedInsn::parse(&text);
        
        // 目标是追踪这个行产生的 x20，所以我们直接将这行视为污点源头
        if parsed.write_regs.iter().any(|(r, _)| r == target_reg) {
            start_line = Some(target_line);
        } else if parsed.read_regs.iter().any(|(r, _)| r == target_reg) {
            // 如果它只是读取了 x20，我们就往回找它的写入点
            let mut current_line = target_line.saturating_sub(1);
            while current_line > 0 {
                if let Some(line_text) = service.get_line_text(current_line) {
                    let prev_parsed = crate::insn_analyzer::ParsedInsn::parse(&line_text);
                    if prev_parsed.write_regs.iter().any(|(r, _)| r == target_reg) {
                        start_line = Some(current_line);
                        break;
                    }
                }
                current_line -= 1;
            }
        }
    }
    
    if let Some(start) = start_line {
        println!("\n=== 以 target_line: 行 {} (写 {}) 为起点进行后向切片 ===", start + 1, target_reg);
        
        // 我们以这个找到的定义行为起点，进行后向切片
        let slice_result = slicer::backward_slice(&state, &[start]);
        
        let mut sorted_lines: Vec<_> = slice_result.marked_lines.iter().copied().collect();
        sorted_lines.sort_unstable();
        
        println!("切片包含 {} 行，占总行数的 {:.2}%", 
                 sorted_lines.len(), 
                 (sorted_lines.len() as f64 / state.line_count as f64) * 100.0);

        println!("\n切片轨迹:");
        for &line in &sorted_lines {
            if let Some(text) = service.get_line_text(line) {
                println!("  [Line {}]: {}", line + 1, text);
            }
        }

        // 渲染重构后的 DAG，可以选择是否开启 Pass-Through 剪枝
        slice_result.render_dag(&mut service, true);
    } else {
        println!("未找到目标寄存器在指定行的相关定义");
    }

    Ok(())
}
