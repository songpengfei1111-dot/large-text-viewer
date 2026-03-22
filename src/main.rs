mod cli_core;
mod search_service;
mod insn_analyzer;
mod summery_analyzer;
mod build_call_tree_forward;
mod taint;
mod taint_by_search;
mod gum_taint;
mod build_call_tree;

fn main() {
    // 测试正则分析引擎 (带有数据剪枝与可视化重构)
    // let _ = build_call_tree_forward::test_build_call_tree();
    let _ = build_call_tree::test_build_call_tree();

    // 运行最简原型Def-Use扫描和切片测试 
    // if let Err(e) = taint::test_def_use() {
    //     eprintln!("Error: {}", e);
    // }
    
    // if let Err(e) = taint_by_search::test_taint_by_search() {
    //     eprintln!("Error: {}", e);
    // }
    
    // if let Err(e) = gum_taint::test_gum_taint() {
    //     eprintln!("Error: {}", e);
    // }
}

