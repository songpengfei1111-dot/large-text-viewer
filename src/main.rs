mod cli_core;
mod taint_engine;
mod search_service;
mod insn_analyzer;
mod trace_path_tree;
mod summery_analyzer;
// 新增

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
    // 运行演示
    // taint_demo::demo_shadow_memory();
    // taint_demo::demo_insn_analyzer();
    // taint_demo::demo_full_taint_flow();
    
    // 运行实际的污点追踪
    // let _ = taint_engine::test_taint();
    // let _ = taint_engine::test_taint_1();
    let _ = taint_engine::test_taint_overlap();


    // 测试 agf_render 功能
    // test_agf_render();
    
    // 测试 summery_analyzer 功能
    // test_summery_analyzer();
}

fn test_summery_analyzer() {
    use crate::summery_analyzer::AssemblyAnalyzer;
    
    println!("=== 测试 summery_analyzer ===");
    
    match AssemblyAnalyzer::new("logs/record_01.csv") {
        Ok(analyzer) => {
            println!("总指令数: {}", analyzer.get_total_instructions());
            println!("唯一指令数: {}", analyzer.get_opcode_count());
            
            println!("\n最频繁的 10 条指令:");
            for (opcode, count) in analyzer.get_opcode_frequency(10) {
                println!("  {}: {}", opcode, count);
            }
            
            println!("\n指令分布 (前 10 条):");
            for (opcode, count, percentage) in analyzer.get_operation_distribution().into_iter().take(10) {
                println!("  {}: {} ({:.2}%)", opcode, count, percentage);
            }
            
            println!("\n内存操作指令数: {}", analyzer.get_memory_operations().len());
            println!("分支操作指令数: {}", analyzer.get_branch_operations().len());
            
            println!("\n=== 测试完成 ===");
        }
        Err(e) => {
            eprintln!("错误: {}", e);
        }
    }
}




/// 测试 agf_render 的 CFG 渲染功能
/// 注意不能出现中文文本，不然长度计算回出现误差
fn test_agf_render() {
    use agf_render::{Graph, EdgeColor, layout, render_to_stdout};

    println!("=== 测试 agf_render CFG 渲染 ===\n");

    // 创建一个简单的控制流图
    let mut g = Graph::new();

    // 添加节点
    let entry = g.add_node(
        "entry",
        "push rbp\n\
        mov rbp, rsp\n\
        cmp eax, 0\n\
        je false_branch",
    );
    let true_branch = g.add_node(
        "true_branch",
        "mov eax, 1\n\
        jmp exit",
    );
    let false_branch = g.add_node(
        "false_branch",
        "mov eax, 0 asdlfjasdfasdjhjkhkhk",
    );
    let exit = g.add_node(
        "exit",
        "pop rbp\nret",
    );

    // 添加边
    g.add_edge(entry, true_branch, EdgeColor::False);  // 不跳转
    g.add_edge(entry, false_branch, EdgeColor::True);  // 跳转
    // g.add_edge_uncond(true_branch, exit);
    g.add_edge(true_branch, exit, EdgeColor::True);
    g.add_edge(true_branch, true_branch, EdgeColor::False);

    g.add_edge_uncond(false_branch, exit);


    // 执行布局算法
    layout(&mut g);

    // 渲染到标准输出
    render_to_stdout(&g);

    println!("\n=== agf_render 测试完成 ===");
}
