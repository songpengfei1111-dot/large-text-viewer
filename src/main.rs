mod cli_core;
mod taint_engine;
mod search_service;
mod insn_analyzer;
mod summery_analyzer;
mod build_call_tree;
mod taint;

fn main() {
    // 原始测试 (保留现有全链路逻辑)
    // let _ = taint_engine::test_taint_overlap();

    // 运行最简原型Def-Use扫描和切片测试 (带有数据剪枝与可视化重构)
    if let Err(e) = taint::test_def_use() {
        eprintln!("Error: {}", e);
    }
}
