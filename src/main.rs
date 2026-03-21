mod cli_core;
mod search_service;
mod insn_analyzer;
mod summery_analyzer;
mod build_call_tree;
mod taint;

fn main() {
    // 测试正则分析引擎 (带有数据剪枝与可视化重构)
    // let _ = taint_engine::test_taint_overlap();

    // 运行最简原型Def-Use扫描和切片测试 
    if let Err(e) = taint::test_def_use() {
        eprintln!("Error: {}", e);
    }
}
