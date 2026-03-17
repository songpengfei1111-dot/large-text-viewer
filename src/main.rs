mod cli_core;
mod taint_engine;
mod search_service;
mod insn_analyzer;
mod trace_path_tree;
mod summery_analyzer;
mod build_call_tree;

fn main() {
    // build_call_tree::test_build_call_tree();
    let _ = taint_engine::test_taint_new_arch();
}
