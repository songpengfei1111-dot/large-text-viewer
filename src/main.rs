mod cli_core;
mod taint_engine;
mod search_service;
mod insn_analyzer;
mod trace_path_tree;
mod summery_analyzer;
mod improved_call_tree;
mod call_tree_integration;
mod debug_call_tree;
mod build_call_tree;

fn main() {
    debug_call_tree::debug_all_instructions();
}
