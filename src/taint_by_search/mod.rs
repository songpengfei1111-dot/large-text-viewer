pub mod engine;

use crate::search_service::SearchService;
use large_text_core::file_reader::FileReader;
use std::path::PathBuf;
use engine::{TaintBySearchEngine, TaintTarget};

pub fn test_taint_by_search() -> anyhow::Result<()> {
    let file_path = PathBuf::from("/Users/bytedance/RustroverProjects/logs/record_01.csv");
    let reader = FileReader::new(file_path, encoding_rs::UTF_8)?;
    let mut service = SearchService::new(reader);

    let mut engine = TaintBySearchEngine::new(&mut service, 50);

    // ldr x20, [sp, #0xc08] 读取 0x6cf01586a8 (8字节) -> 假设我们要追踪 x20
    // 行号: 9028 (CSV中 0-based 索引为 9027)
    let start_line = 9027;
    let target = TaintTarget::Reg("x20".to_string());
    
    println!("=== 开始 taint_by_search 追踪 ===");
    println!("追踪目标: {} 从行 {}", target, start_line + 1);
    
    let chain = engine.trace(start_line, vec![target]);
    
    println!("=== 追踪结果 ===");
    for (i, step) in chain.iter().enumerate() {
        let hits = step.hit_targets.iter().map(|t| t.to_string()).collect::<Vec<_>>().join(", ");
        let news = step.new_targets.iter().map(|t| t.to_string()).collect::<Vec<_>>().join(", ");
        
        println!("[Step {}] 行 {}: {}", i + 1, step.line_num + 1, step.instruction);
        println!("  - 命中目标: {}", hits);
        println!("  - 新增追踪: {}", news);
    }
    
    Ok(())
}
