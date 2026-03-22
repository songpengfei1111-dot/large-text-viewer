use std::collections::{HashSet, VecDeque, HashMap};
use crate::taint::scanner::{ScanState, DepNode};
use crate::search_service::SearchService;
use crate::insn_analyzer::{ParsedInsn, InsnType};
use agf_render::{Graph, EdgeColor, layout, render_to_stdout, render_to_svg};

pub struct SliceResult {
    pub marked_lines: HashSet<usize>,
    pub edges: Vec<(usize, usize)>, // (from, to) -> from line depends on to line
}

impl SliceResult {
    /// 渲染 DAG
    pub fn render_dag(&self, service: &mut SearchService, prune_pass_through: bool) {
        let mut final_edges = self.edges.clone();
        let mut final_nodes = self.marked_lines.clone();

        if prune_pass_through {
            // 进行 pass-through 剪枝：
            // 如果节点 B 是简单的 Store/Move 指令，且不产生新的算术变化，
            // 我们可以将 A -> B -> C 压缩为 A -> C
            let mut node_insn_types = HashMap::new();
            for &line in &final_nodes {
                if let Some(text) = service.get_line_text(line) {
                    let parsed = ParsedInsn::parse(&text);
                    node_insn_types.insert(line, parsed.insn_type);
                }
            }

            let mut to_remove = HashSet::new();
            
            // 找出所有可以短接的节点 B
            for &b in &final_nodes {
                if let Some(insn_type) = node_insn_types.get(&b) {
                    // 如果是 Store 或者单纯的数据转移 (Other)，可以考虑剪枝
                    if *insn_type == InsnType::Store || *insn_type == InsnType::Other {
                        to_remove.insert(b);
                    }
                }
            }

            // 对每一条 A -> B，如果 B 被剪枝，且 B -> C，则建立 A -> C
            let mut adj_out: HashMap<usize, Vec<usize>> = HashMap::new(); // node -> deps
            let mut adj_in: HashMap<usize, Vec<usize>> = HashMap::new();  // node -> dependent by

            for &(from, to) in &final_edges {
                adj_out.entry(from).or_default().push(to);
                adj_in.entry(to).or_default().push(from);
            }

            for &b in &to_remove {
                let parents = adj_in.get(&b).cloned().unwrap_or_default();
                let children = adj_out.get(&b).cloned().unwrap_or_default();

                for &a in &parents {
                    // a 不再依赖 b
                    if let Some(deps) = adj_out.get_mut(&a) {
                        deps.retain(|&x| x != b);
                        // a 继承 b 的依赖 c
                        for &c in &children {
                            if !deps.contains(&c) {
                                deps.push(c);
                            }
                        }
                    }
                }
                
                for &c in &children {
                    // c 不再被 b 依赖
                    if let Some(deps) = adj_in.get_mut(&c) {
                        deps.retain(|&x| x != b);
                        // c 被 a 依赖
                        for &a in &parents {
                            if !deps.contains(&a) {
                                deps.push(a);
                            }
                        }
                    }
                }

                // 从图中移除 B
                final_nodes.remove(&b);
            }

            // 重建边集合
            final_edges.clear();
            for (&from, tos) in &adj_out {
                if final_nodes.contains(&from) {
                    for &to in tos {
                        if final_nodes.contains(&to) {
                            final_edges.push((from, to));
                        }
                    }
                }
            }
        }

        let mut graph = Graph::new();
        let mut node_ids = HashMap::new();

        let mut sorted_lines: Vec<_> = final_nodes.into_iter().collect();
        sorted_lines.sort_unstable();

        for line in sorted_lines {
            if let Some(text) = service.get_line_text(line) {
                let parts: Vec<&str> = text.split(';').collect();
                let insn_name = parts.get(3).unwrap_or(&"").trim();
                let insn_opt = parts.get(4).unwrap_or(&"").trim();
                
                let display_text = format!("{}:{} {}", line + 1, insn_name, insn_opt);
                let display_text = if display_text.chars().count() > 45 {
                    format!("{}...", display_text.chars().take(42).collect::<String>())
                } else {
                    display_text
                };

                let node_id = graph.add_node(&format!("{}", line), &display_text);
                node_ids.insert(line, node_id);
            }
        }

        // 添加边 (from_id -> to_id, 意味着依赖流向)
        // 渲染时，通常我们希望看到数据流动，即 source -> target，所以是 to -> from
        for (from, to) in final_edges {
            if let (Some(&from_id), Some(&to_id)) = (node_ids.get(&from), node_ids.get(&to)) {
                // 箭头方向：被依赖者 -> 依赖者 (数据流动方向)
                graph.add_edge(to_id, from_id, EdgeColor::True);
            }
        }

        println!("\n=== 渲染追踪路径 (DAG){} ===\n", if prune_pass_through { " - 已剪枝" } else { "" });
        if !node_ids.is_empty() {
            layout(&mut graph);
            render_to_stdout(&graph);

            let svg_content = render_to_svg(&graph);
            if let Err(e) = std::fs::write("taint_output.svg", svg_content) {
                eprintln!("Failed to write taint_output.svg: {}", e);
            } else {
                println!("SVG output written to taint_output.svg");
            }
        } else {
            println!("图为空。");
        }
        println!("\n=== 渲染完成 ===\n");
    }
}

/// 从指定的起点（行号）开始进行后向切片，返回所有相关（被依赖）的行号集合以及边
pub fn backward_slice(state: &ScanState, start_lines: &[usize]) -> SliceResult {
    let mut marked_lines = HashSet::new();
    let mut visited_nodes = HashSet::new();
    let mut queue: VecDeque<DepNode> = VecDeque::new();
    let mut edges = HashSet::new();

    for &line in start_lines {
        if line < state.line_count {
            let node = DepNode::Line(line);
            if visited_nodes.insert(node) {
                queue.push_back(node);
                marked_lines.insert(line);
            }
        }
    }

    while let Some(node) = queue.pop_front() {
        let line = node.line();
        if line >= state.deps.len() {
            continue;
        }

        let deps = &state.deps[line];
        let mut deps_to_follow = Vec::new();

        deps_to_follow.extend(deps.normal.iter().copied());

        match node {
            DepNode::Line(_) => {
                deps_to_follow.extend(deps.half1.iter().copied());
                deps_to_follow.extend(deps.half2.iter().copied());
            }
            DepNode::LineHalf1(_) => {
                deps_to_follow.extend(deps.half1.iter().copied());
            }
            DepNode::LineHalf2(_) => {
                deps_to_follow.extend(deps.half2.iter().copied());
            }
        }

        for dep_node in deps_to_follow {
            let dep_line = dep_node.line();
            edges.insert((line, dep_line)); // line depends on dep_line

            if visited_nodes.insert(dep_node) {
                queue.push_back(dep_node);
                marked_lines.insert(dep_line);
            }
        }
    }

    SliceResult {
        marked_lines,
        edges: edges.into_iter().collect(),
    }
}
