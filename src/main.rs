mod cli_core;
mod taint_engine;
mod search_service;
mod shadow_memory;    // 新增
mod insn_analyzer;    // 新增

// use std::env;
// fn main() -> eframe::Result<()> {
    // 检查是否有命令行参数
    // let _args: Vec<String> = env::args().collect();
    // match cli_core::run_cli() {
    //     Ok(()) => return Ok(()),
    //     Err(e) => {
    //         eprintln!("CLI Error: {}", e);
    //         std::process::exit(1);
    //     }
    // }
// }

fn main() {
    // 运行演示
    // taint_demo::demo_shadow_memory();
    // taint_demo::demo_insn_analyzer();
    // taint_demo::demo_full_taint_flow();
    
    // 运行实际的污点追踪
    // let _ = taint_engine::test_taint();
    // let _ = taint_engine::test_taint_1();
    let _ = taint_engine::test_taint_overlap();
    
    // 其他测试
    // test_reg::test_reg();
    // insn_il::test_parse_single();
    // insn_il::test_parse_instruction();

}
