mod cli_core;
mod taint_engine;
mod search_service;
mod insn_analyzer;
mod summery_analyzer;
mod build_call_tree;
mod tree;

fn main() {
    let _ = taint_engine::test_taint_overlap();
}
