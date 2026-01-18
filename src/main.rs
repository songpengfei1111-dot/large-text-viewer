// mod app;
mod app_simp;
mod cli_core;

// use app::TextViewerApp;
use app_simp::TextViewerAppSimp;
use eframe::egui;
use std::env;

fn main() -> eframe::Result<()> {
    // 检查是否有命令行参数
    let args: Vec<String> = env::args().collect();

    // 如果有命令行参数且不是GUI模式，运行CLI
    if args.len() > 1 && !args.contains(&"--gui".to_string()) {
        match cli_core::run_cli() {
            Ok(()) => return Ok(()),
            Err(e) => {
                eprintln!("CLI Error: {}", e);
                std::process::exit(1);
            }
        }
    }

    // 否则运行GUI
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_title("Large Text Viewer"),
        ..Default::default()
    };
    //
    // eframe::run_native(
    //     "Large Text Viewer",
    //     options,
    //     Box::new(|_cc| Ok(Box::new(TextViewerApp::default()))),
    // )

    eframe::run_native(
        "Large Text Viewer",
        options,
        Box::new(|_cc| Ok(Box::new(TextViewerAppSimp::default()))),
    )


}

