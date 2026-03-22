pub mod category;
pub mod parser;
pub mod engine;

use crate::search_service::SearchService;
use large_text_core::file_reader::FileReader;
use std::path::PathBuf;
use engine::{TaintEngine, TrackMode, TaintSource};
use parser::TraceLine;

pub fn test_gum_taint() -> anyhow::Result<()> {
    let file_path = PathBuf::from("/Users/bytedance/RustroverProjects/logs/record_01.csv");
    let reader = FileReader::new(file_path, encoding_rs::UTF_8)?;
    let mut service = SearchService::new(reader);

    // 假设同样追踪行 9027 的 x20
    let start_line = 9027;
    let target_reg = category::parse_reg_name("x20");
    
    println!("=== 开始 GumTrace Taint Backward 追踪 ===");
    
    let source = TaintSource::from_reg(target_reg);
    let mut engine = TaintEngine::new(TrackMode::Backward, source);
    
    let mut current_line = start_line;
    let mut lines_since_last_propagation = 0;
    
    // 我们先解析起点
    if let Some(text) = service.get_line_text(current_line) {
        if let Some(line) = TraceLine::parse(current_line, &text) {
            engine.process_line(&line); // 处理起点
        }
    }
    
    while current_line > 0 {
        current_line -= 1;
        
        if let Some(text) = service.get_line_text(current_line) {
            if let Some(line) = TraceLine::parse(current_line, &text) {
                let involved = engine.process_line(&line);
                
                if involved {
                    lines_since_last_propagation = 0;
                } else {
                    lines_since_last_propagation += 1;
                    if lines_since_last_propagation >= engine.max_scan_distance {
                        engine.stop_reason = engine::StopReason::ScanLimitReached;
                        break;
                    }
                }
                
                if engine.stop_reason == engine::StopReason::AllTaintCleared {
                    break;
                }
            }
        }
    }
    
    println!("追踪停止原因: {:?}", engine.stop_reason);
    println!("命中的指令数: {}", engine.results.len());
    
    // 反转 results 使得结果按时间正序输出
    let mut final_results = engine.results;
    final_results.reverse();
    
    for res in final_results {
        println!("[{}] {}", res.index + 1, res.raw_text);
        
        // 打印快照
        let mut tainted_regs = Vec::new();
        for i in 0..256 {
            if res.reg_snapshot[i] {
                tainted_regs.push(category::reg_name(i));
            }
        }
        let tainted_mems: Vec<String> = res.mem_snapshot.iter().map(|a| format!("mem:0x{:x}(size:{})", a.addr, a.size)).collect();
        
        println!("      tainted: regs={:?} mems={:?}", tainted_regs, tainted_mems);
    }

    Ok(())
}
