mod test_reg;

mod cli_core;
mod taint_engine;
mod search_service;  // 添加这一行

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
    let _ = taint_engine::test_taint();
    // test_reg::test_reg();
}
