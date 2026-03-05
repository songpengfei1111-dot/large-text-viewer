mod test_reg;

mod cli_core;
mod taint_engine;
mod search_service;  // 添加这一行

mod insn_il;
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
    // let _ = taint_engine::test_taint();
    let _ = taint_engine::test_taint_1();
    // test_reg::test_reg();
    // insn_il::test_parse_single();
    // insn_il::test_parse_instruction();

}
