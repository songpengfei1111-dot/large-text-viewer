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
    // let _ = taint_engine::test_taint();
    test_reg::test_reg();
}



pub fn test_search_count() -> anyhow::Result<()> {
    use large_text_core::file_reader::FileReader;
    use crate::search_service::{SearchService, SearchConfig};

    let file_path = std::path::PathBuf::from("/Users/teng/RustroverProjects/large-text-viewer/logs/record.csv");
    let reader = FileReader::new(file_path, encoding_rs::UTF_8)?;
    let service = SearchService::new(reader); // 会自动index

    let pattern = "ld__6cf01586a0_".to_string();
    let config = SearchConfig::new(pattern.clone())
        .with_regex(false)
        .with_max_results(99)
        .with_context(0)
        .with_line_range(None, None);

    // 搜索模式
    let summary = service.search(config)?;
    for m in &summary.matches {
        println!("{}:{}",m.line_number + 1, m.line_text);
    }
    println!("\nShowed {} matches", summary.total_matches);


    // get_line
    let line_num = 9029;
    let line_text = service.get_line_text(line_num-1).unwrap_or("not found".to_string());
    println!("{}:{}", line_num,line_text);

    //
    let config = SearchConfig::new("st__6cf01586a0_".to_string())
        .with_regex(true)
        .with_context(0)  // 显示前后2行上下文
        .with_case_sensitive(true);
    let result = service.find_prev(line_num-1,config).unwrap();

    println!("{}:{}", result.line_number,result.line_text);
    // 然后按指令case解析，再下发搜索命令

    let config = SearchConfig::new("q0=0x0x0100000001000000e061f2cb6c000000".to_string())
        .with_regex(true)
        .with_context(0)  // 显示前后2行上下文
        .with_case_sensitive(true);;
    let result = service.find_prev(result.line_number-1,config).unwrap();
    println!("{}:{}", result.line_number,result.line_text);


    let config = SearchConfig::new("st__6f5c29c520".to_string())
        .with_regex(true)
        .with_context(0)  // 显示前后2行上下文
        .with_case_sensitive(true);;
    let result = service.find_prev(result.line_number-1,config).unwrap();
    println!("{}:{}", result.line_number,result.line_text);

    Ok(())
}

//TODO reader持久化
pub fn mem2reg(){

}

pub fn reg2mem(){

}

pub fn algop(){

}

//TODO 按照指令判断op类型
// 自动xref
