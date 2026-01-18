mod cli_core;

// use app::TextViewerApp;
use eframe::egui;
use std::env;

fn main() -> eframe::Result<()> {
    // 检查是否有命令行参数
    let args: Vec<String> = env::args().collect();
    match cli_core::run_cli() {
        Ok(()) => return Ok(()),
        Err(e) => {
            eprintln!("CLI Error: {}", e);
            std::process::exit(1);
        }
    }

}

