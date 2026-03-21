use std::collections::{HashSet, VecDeque};
use crate::taint::scanner::{ScanState, DepNode};

/// 从指定的起点（行号）开始进行后向切片，返回所有相关（被依赖）的行号集合
pub fn backward_slice(state: &ScanState, start_lines: &[usize]) -> HashSet<usize> {
    let mut marked_lines = HashSet::new();
    let mut visited_nodes = HashSet::new();
    let mut queue: VecDeque<DepNode> = VecDeque::new();

    // 初始化队列，默认将起点的 Line 节点入队
    for &line in start_lines {
        if line < state.line_count {
            let node = DepNode::Line(line);
            if visited_nodes.insert(node) {
                queue.push_back(node);
                marked_lines.insert(line);
            }
        }
    }

    // BFS 反向遍历依赖图
    while let Some(node) = queue.pop_front() {
        let line = node.line();
        if line >= state.deps.len() {
            continue;
        }

        let deps = &state.deps[line];
        let mut deps_to_follow = Vec::new();

        // 无论哪种到达路径，都需要加上 shared (normal) 的依赖
        deps_to_follow.extend(deps.normal.iter().copied());

        match node {
            DepNode::Line(_) => {
                // 如果是对整个行的依赖，需要加上所有半区的依赖
                deps_to_follow.extend(deps.half1.iter().copied());
                deps_to_follow.extend(deps.half2.iter().copied());
            }
            DepNode::LineHalf1(_) => {
                // 如果只依赖第一半区，只追加 half1 的依赖
                deps_to_follow.extend(deps.half1.iter().copied());
            }
            DepNode::LineHalf2(_) => {
                // 如果只依赖第二半区，只追加 half2 的依赖
                deps_to_follow.extend(deps.half2.iter().copied());
            }
        }

        for dep_node in deps_to_follow {
            if visited_nodes.insert(dep_node) {
                queue.push_back(dep_node);
                marked_lines.insert(dep_node.line());
            }
        }
    }

    marked_lines
}
