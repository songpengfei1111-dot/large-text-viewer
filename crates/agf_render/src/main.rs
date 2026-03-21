/// agf_render —— Rizin agf 图形渲染器的 Rust 重写版
///
/// 架构说明（对应 rizin 原版各组件）：
///
/// ┌─────────────────────────────────────────────────────────┐
/// │  main.rs       构造图数据 (RzANode + 边)               │
/// │  graph.rs      Graph / Node / Edge 数据结构             │
/// │  layout.rs     Sugiyama 分层布局算法 (set_layout)       │
/// │  render.rs     节点/边绘制 (agraph_print_nodes/edges)   │
/// │  canvas.rs     2D 字符画布 (RzConsCanvas)              │
/// └─────────────────────────────────────────────────────────┘

mod canvas;
mod graph;
mod layout;
mod render;
mod svg_render;

use graph::{EdgeColor, Graph};

fn main() {
    // ── 示例 1：简单条件跳转 CFG ──────────────────────────────
    println!("=== 示例 1：条件跳转 CFG ===\n");
    let mut g = Graph::new();

    let entry = g.add_node(
        "0x1000",
        "push rbp\n\
         mov  rbp, rsp\n\
         sub  rsp, 0x10\n\
         mov  DWORD PTR [rbp-0x4], 0\n\
         cmp  DWORD PTR [rbp-0x4], 0\n\
         je   0x1030",
    );
    let true_branch = g.add_node(
        "0x1010",
        "mov  eax, 1\n\
         jmp  0x1040",
    );
    let false_branch = g.add_node(
        "0x1030",
        "mov  eax, 0",
    );
    let exit = g.add_node(
        "0x1040",
        "leave\n\
         ret",
    );

    // 条件跳转边（true = 绿色，false = 红色）
    g.add_edge(entry, true_branch, EdgeColor::False); // je 不跳 → false
    g.add_edge(entry, false_branch, EdgeColor::True); // je 跳   → true
    g.add_edge_uncond(true_branch, exit);
    g.add_edge_uncond(false_branch, exit);

    layout::layout(&mut g);
    render::render_to_stdout(&g);

    // ── 示例 2：带循环的 CFG（含回边） ───────────────────────
    println!("\n\n=== 示例 2：循环结构（含回边）===\n");
    let mut g2 = Graph::new();

    let header = g2.add_node(
        "0x2000",
        "mov  ecx, 0\n\
         jmp  0x2010",
    );
    let loop_body = g2.add_node(
        "0x2010",
        "cmp  ecx, 10\n\
         jge  0x2030",
    );
    let loop_inc = g2.add_node(
        "0x2020",
        "add  ecx, 1\n\
         jmp  0x2010",
    );
    let loop_exit = g2.add_node(
        "0x2030",
        "mov  eax, ecx\n\
         ret",
    );

    g2.add_edge_uncond(header, loop_body);
    g2.add_edge(loop_body, loop_inc, EdgeColor::False); // 未达到 10 → 继续循环
    g2.add_edge(loop_body, loop_exit, EdgeColor::True); // >= 10 → 退出
    g2.add_edge_uncond(loop_inc, loop_body); // 回边

    layout::layout(&mut g2);
    render::render_to_stdout(&g2);

    // ── 示例 3：线性序列（无分支）─────────────────────────────
    println!("\n\n=== 示例 3：调用链（线性图）===\n");
    let mut g3 = Graph::new();

    let n0 = g3.add_node("main", "call setup\ncall process\ncall cleanup\nret");
    let n1 = g3.add_node("setup", "push rbp\n...\nret");
    let n2 = g3.add_node("process", "push rbp\n...\nret");
    let n3 = g3.add_node("cleanup", "push rbp\n...\nret");

    g3.add_edge_uncond(n0, n1);
    g3.add_edge_uncond(n1, n2);
    g3.add_edge_uncond(n2, n3);

    layout::layout(&mut g3);
    render::render_to_stdout(&g3);

    // ── 示例 4：复杂 CFG（嵌套 if-else + 循环 + 多汇聚）─────────
    // 模拟一个较复杂的函数：
    //
    //   entry(0x3000)
    //      │
    //   cond1(0x3010)  ← 条件判断
    //    ┌──┴──┐
    // if_body  else_body
    // (0x3020) (0x3030)
    //    │        │
    //    │     cond2(0x3040) ← else 内部再判断
    //    │      ┌──┴──┐
    //    │   inner_t inner_f
    //    │   (0x3050)(0x3060)
    //    │      └──┬──┘
    //    └────merge(0x3070) ← 多分支汇聚
    //          │
    //       loop_hdr(0x3080) ← 循环头
    //        ┌──┴──┐
    //     loop_body loop_exit
    //     (0x3090)  (0x30b0)
    //        │
    //     loop_tail(0x30a0)
    //        └──→ loop_hdr (回边)
    //
    println!("\n\n=== 示例 4：复杂 CFG（嵌套分支 + 循环 + 多汇聚）===\n");
    let mut g4 = Graph::new();

    let entry = g4.add_node(
        "0x3000",
        "push rbp\n\
         mov  rbp, rsp\n\
         sub  rsp, 0x20",
    );
    let cond1 = g4.add_node(
        "0x3010",
        "cmp  DWORD PTR [rbp-0x4], 0\n\
         jle  0x3030",
    );
    let if_body = g4.add_node(
        "0x3020",
        "mov  eax, [rbp-0x8]\n\
         add  eax, 1\n\
         mov  [rbp-0x8], eax\n\
         jmp  0x3070",
    );
    let else_body = g4.add_node(
        "0x3030",
        "mov  eax, [rbp-0xc]\n\
         test eax, eax\n\
         je   0x3060",
    );
    let cond2 = g4.add_node(
        "0x3040",
        "cmp  eax, 0xff\n\
         jge  0x3060",
    );
    let inner_true = g4.add_node(
        "0x3050",
        "shl  eax, 2\n\
         jmp  0x3070",
    );
    let inner_false = g4.add_node(
        "0x3060",
        "xor  eax, eax",
    );
    let merge = g4.add_node(
        "0x3070",
        "mov  [rbp-0x10], eax\n\
         mov  ecx, 0",
    );
    let loop_hdr = g4.add_node(
        "0x3080",
        "cmp  ecx, [rbp-0x10]\n\
         jge  0x30b0",
    );
    let loop_body = g4.add_node(
        "0x3090",
        "mov  edx, [rbp+ecx*4]\n\
         add  edx, eax\n\
         add  edx, eax\n\
         add  edx, eax\n\
         add  edx, eax\n\
         add  edx, eax\n\
         add  edx, eax\n\
         add  edx, eax\n\
         mov  [rbp+ecx*4], edx",
    );
    let loop_tail = g4.add_node(
        "0x30a0",
        "inc  ecx\n\
         inc  ecx\n\
         inc  ecx\n\
         inc  ecx\n\
         inc  ecx\n\
         inc  ecx\n\
         inc  ecx\n\
         inc  ecx\n\
         inc  ecx\n\
         inc  ecx\n\
         inc  ecx\n\
         inc  ecx\n\
         inc  ecx\n\
         inc  ecx\n\
         jmp  0x3080",
    );
    let loop_exit = g4.add_node(
        "0x30b0",
        "mov  eax, [rbp-0x10]\n\
         leave\n\
         ret",
    );

    // entry → cond1
    g4.add_edge_uncond(entry, cond1);
    // cond1: true(>0) → if_body, false(<=0) → else_body
    g4.add_edge(cond1, if_body, EdgeColor::True);
    g4.add_edge(cond1, else_body, EdgeColor::False);
    // if_body → merge
    g4.add_edge_uncond(if_body, merge);
    // else_body → cond2
    g4.add_edge_uncond(else_body, cond2);
    // cond2: true(<0xff) → inner_true, false(>=0xff) → inner_false
    g4.add_edge(cond2, inner_true, EdgeColor::True);
    g4.add_edge(cond2, inner_false, EdgeColor::False);
    // inner_true → merge, inner_false → merge
    g4.add_edge_uncond(inner_true, merge);
    g4.add_edge_uncond(inner_false, merge);
    // merge → loop_hdr
    g4.add_edge_uncond(merge, loop_hdr);
    // loop_hdr: true(< limit) → loop_body, false(>= limit) → loop_exit
    g4.add_edge(loop_hdr, loop_body, EdgeColor::True);
    g4.add_edge(loop_hdr, loop_exit, EdgeColor::False);
    // loop_body → loop_tail
    g4.add_edge_uncond(loop_body, loop_tail);
    // loop_tail → loop_hdr  (回边！)
    g4.add_edge_uncond(loop_tail, loop_hdr);

    layout::layout(&mut g4);
    render::render_to_stdout(&g4);

    let svg_content = svg_render::render_to_svg(&g4);
    std::fs::write("output.svg", svg_content).unwrap();
    println!("SVG output written to output.svg");
}
