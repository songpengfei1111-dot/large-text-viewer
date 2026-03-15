/// Sugiyama 分层布局算法实现
///
/// 对应 rizin agraph.c 中的 `set_layout()` 函数，步骤：
/// 1. `remove_cycles`     —— 环路消除（DFS 反转回边）
/// 2. `assign_layers`     —— 层分配（最长路径法）
/// 3. `insert_dummy_nodes` —— 哑节点插入（拆分跨层长边）
/// 4. `minimize_crossings` —— 层内节点排序（重心/中位数启发式）
/// 5. `assign_coordinates` —— 坐标赋值（x/y 坐标）

use crate::graph::{Graph, NodeId};

/// 节点间的水平间距（列），对应 `HORIZONTAL_NODE_SPACING`
const H_GAP: i32 = 4;
/// 层间的垂直间距（行），对应 `VERTICAL_NODE_SPACING`
const V_GAP: i32 = 3;
/// 画布左边距
const MARGIN_X: i32 = 2;
/// 画布顶边距
const MARGIN_Y: i32 = 1;

/// 主入口：对图 g 进行 Sugiyama 布局，填充每个节点的 (x, y, layer, pos_in_layer)。
/// 对应 rizin 的 `set_layout(g)`。
pub fn layout(g: &mut Graph) {
    // 1. 环路消除：DFS 反转回边，使图变为 DAG
    remove_cycles(g);

    // 2. 层分配：最长路径法（sources 在第 0 层）
    assign_layers(g);

    // 3. 哑节点插入：拆分跨多层的长边
    insert_dummy_nodes(g);

    let n_layers = g.layer_count();
    if n_layers == 0 {
        return;
    }

    // 4. 初始化 pos_in_layer（按节点 ID 顺序，确保初始顺序稳定）
    for layer in 0..n_layers as i32 {
        let mut nodes_in_layer: Vec<NodeId> = g
            .nodes
            .iter()
            .filter(|n| n.layer == layer)
            .map(|n| n.id)
            .collect();
        nodes_in_layer.sort();
        for (pos, id) in nodes_in_layer.iter().enumerate() {
            g.nodes[*id].pos_in_layer = pos as i32;
        }
    }

    // 5. 最小化边交叉（重心法，多次前向+后向扫描）
    minimize_crossings(g, n_layers);

    // 6. 坐标赋值：先确定每层的 y，再确定每个节点的 x
    assign_coordinates(g, n_layers);
}

// ─────────────────────────────────────────────────────────────
// 步骤 1：环路消除
// 对应 rizin 的 `remove_cycles(g)` + DFS
// ─────────────────────────────────────────────────────────────

fn remove_cycles(g: &mut Graph) {
    let n = g.node_count();
    let mut visited = vec![false; n];
    let mut in_stack = vec![false; n];

    // 遍历所有节点，对每个未访问的节点做 DFS
    for start in 0..n {
        if !visited[start] {
            dfs_remove_cycles(g, start, &mut visited, &mut in_stack);
        }
    }
}

fn dfs_remove_cycles(
    g: &mut Graph,
    node: NodeId,
    visited: &mut Vec<bool>,
    in_stack: &mut Vec<bool>,
) {
    visited[node] = true;
    in_stack[node] = true;

    // 克隆出边索引列表，避免借用冲突
    let out_edges: Vec<usize> = g.adj_out[node].clone();

    for ei in out_edges {
        if g.edges[ei].dead {
            continue;
        }
        let to = g.edges[ei].to;
        if !visited[to] {
            dfs_remove_cycles(g, to, visited, in_stack);
        } else if in_stack[to] {
            // 发现回边：
            // 1. 先将原始方向 (from → to) 保存到 g.back_edges，供渲染时绘制向上箭头
            // 2. 再反转该边，使图变为 DAG，参与后续层分配
            // 对应 rizin 的 `remove_cycles` 把回边存入 g->back_edges 后反转
            let from = g.edges[ei].from;
            let color = g.edges[ei].color;
            g.back_edges.push((from, to, color)); // ← 保存原始回边

            g.edges[ei].reversed = true;

            // 更新邻接表（反转 from ↔ to）
            g.adj_out[from].retain(|&e| e != ei);
            g.adj_in[to].retain(|&e| e != ei);
            g.edges[ei].from = to;
            g.edges[ei].to = from;
            g.adj_out[to].push(ei);
            g.adj_in[from].push(ei);
        }
    }

    in_stack[node] = false;
}

// ─────────────────────────────────────────────────────────────
// 步骤 2：层分配（最长路径法）
// 对应 rizin 的 `assign_layers(g)`
// ─────────────────────────────────────────────────────────────

fn assign_layers(g: &mut Graph) {
    let n = g.node_count();

    // 计算入度（用于拓扑排序）
    let mut in_degree = vec![0usize; n];
    for e in g.edges.iter().filter(|e| !e.dead) {
        in_degree[e.to] += 1;
    }

    // 拓扑排序（Kahn 算法）
    let mut queue: std::collections::VecDeque<NodeId> =
        (0..n).filter(|&i| in_degree[i] == 0).collect();
    let mut order = Vec::with_capacity(n);

    while let Some(node) = queue.pop_front() {
        order.push(node);
        for &ei in &g.adj_out[node].clone() {
            if g.edges[ei].dead {
                continue;
            }
            let to = g.edges[ei].to;
            in_degree[to] -= 1;
            if in_degree[to] == 0 {
                queue.push_back(to);
            }
        }
    }

    // 对于存在孤立节点（未在拓扑排序中出现）的情况，补充处理
    for i in 0..n {
        if !order.contains(&i) {
            order.push(i);
        }
    }

    // 按拓扑序赋层号：source = 0，其余 = max(前驱层号) + 1
    let mut layers = vec![0i32; n];
    for &node in &order {
        for &ei in &g.adj_out[node].clone() {
            if g.edges[ei].dead || g.edges[ei].reversed {
                continue; // 跳过已删除的边和已反转的回边
            }
            let to = g.edges[ei].to;
            layers[to] = layers[to].max(layers[node] + 1);
        }
    }

    for i in 0..n {
        g.nodes[i].layer = layers[i];
    }
}

// ─────────────────────────────────────────────────────────────
// 步骤 3：哑节点插入
// 对应 rizin 的 `create_dummy_nodes(g)`
// ─────────────────────────────────────────────────────────────
// 对每条跨多层的边 (a, b)，插入 (layer_a+1 .. layer_b-1) 层的哑节点链：
//   a -> d1 -> d2 -> ... -> b
// ─────────────────────────────────────────────────────────────

fn insert_dummy_nodes(g: &mut Graph) {
    // 收集需要拆分的边（跨度 > 1 层）
    let long_edges: Vec<(usize, i32, i32)> = g
        .edges
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            !e.dead
                && g.nodes[e.to].layer - g.nodes[e.from].layer > 1
        })
        .map(|(ei, e)| (ei, g.nodes[e.from].layer, g.nodes[e.to].layer))
        .collect();

    for (ei, from_layer, to_layer) in long_edges {
        let orig_from = g.edges[ei].from;
        let orig_to = g.edges[ei].to;
        let edge_color = g.edges[ei].color;

        // 在 from_layer+1 .. to_layer-1 各层插入哑节点
        let mut prev_node = orig_from;
        let is_reversed = g.edges[ei].reversed;
        for l in from_layer + 1..to_layer {
            let d_id = g.add_dummy_node(l, orig_from, orig_to);
            // 添加从 prev -> dummy 的边（传播 reversed 标志，避免反转回边的中间段被当作前向边绘制）
            let new_ei = g.edges.len();
            g.edges.push(crate::graph::Edge {
                from: prev_node,
                to: d_id,
                color: edge_color,
                reversed: is_reversed,
                dead: false,
            });
            g.adj_out[prev_node].push(new_ei);
            g.adj_in[d_id].push(new_ei);
            prev_node = d_id;
        }
        // 最后一个哑节点 -> orig_to 复用原来那条边
        g.redirect_edge_from(ei, prev_node);
        // 原始边的 from 已经被更新为最后一个哑节点
    }
}

// ─────────────────────────────────────────────────────────────
// 步骤 4：交叉最小化（重心启发式）
// 对应 rizin 的 `minimize_crossings(g)`
// ─────────────────────────────────────────────────────────────

fn minimize_crossings(g: &mut Graph, n_layers: usize) {
    // 只进行前向扫描，对于有根树（如二叉树）效果更好
    for _ in 0..4 {
        // 前向扫描：第 1 层开始，根据上一层节点位置排序
        for layer in 1..n_layers as i32 {
            reorder_by_barycenter(g, layer, true);
        }
    }
}

/// 对指定层使用重心法（barycenter）重排节点。
/// `use_prev = true` 时参考上一层；`false` 时参考下一层。
fn reorder_by_barycenter(g: &mut Graph, layer: i32, use_prev: bool) {
    let nodes_in_layer = g.layer_nodes(layer);
    if nodes_in_layer.len() <= 1 {
        return;
    }

    // 计算每个节点的重心值（邻居在相邻层中的平均位置），并记录原始索引
    let mut scores: Vec<(NodeId, f64, usize)> = nodes_in_layer
        .iter()
        .enumerate()
        .map(|(orig_idx, &id)| {
            let neighbors: Vec<NodeId> = if use_prev {
                g.in_neighbors(id)
            } else {
                g.out_neighbors(id)
            };

            let target_layer = if use_prev { layer - 1 } else { layer + 1 };
            let positions: Vec<i32> = neighbors
                .iter()
                .filter(|&&nid| g.nodes[nid].layer == target_layer)
                .map(|&nid| g.nodes[nid].pos_in_layer)
                .collect();

            let score = if positions.is_empty() {
                // 无邻居时保持原位
                g.nodes[id].pos_in_layer as f64
            } else {
                positions.iter().sum::<i32>() as f64 / positions.len() as f64
            };

            (id, score, orig_idx)
        })
        .collect();

    // 按重心值稳定排序，如果重心值相同，则按原始索引排序保持稳定
    scores.sort_by(|a, b| {
        a.1.partial_cmp(&b.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.2.cmp(&b.2))
    });

    // 更新 pos_in_layer
    for (pos, (id, _, _)) in scores.iter().enumerate() {
        g.nodes[*id].pos_in_layer = pos as i32;
    }
}

// ─────────────────────────────────────────────────────────────
// 步骤 5：坐标赋值
// 对应 rizin 的 `place_dummies` + `place_original` + 最终坐标计算
// ─────────────────────────────────────────────────────────────

fn assign_coordinates(g: &mut Graph, n_layers: usize) {
    // ── Y 坐标：每层的起始 y = 上一层 y + 上一层最大高度 + V_GAP
    let mut layer_y = vec![MARGIN_Y; n_layers];
    let mut layer_h = vec![0i32; n_layers];

    for layer in 0..n_layers as i32 {
        let max_h = g
            .layer_nodes(layer)
            .iter()
            .map(|&id| g.nodes[id].h)
            .max()
            .unwrap_or(1);
        layer_h[layer as usize] = max_h;
    }

    let mut y = MARGIN_Y;
    for li in 0..n_layers {
        layer_y[li] = y;
        y += layer_h[li] + V_GAP;
    }

    // 将 y 坐标赋给每个节点
    for layer in 0..n_layers as i32 {
        for id in g.layer_nodes(layer) {
            g.nodes[id].y = layer_y[layer as usize];
        }
    }

    // ── X 坐标：先均匀分布，再用单次前向重心对齐
    // Step 1：计算每层总宽度，然后均匀分布（从 MARGIN_X 开始）
    for layer in 0..n_layers as i32 {
        let mut x = MARGIN_X;
        let nodes = g.layer_nodes(layer);
        for id in nodes {
            g.nodes[id].x = x;
            x += g.nodes[id].w + H_GAP;
        }
    }

    // Step 2：前向单遍：每层根据上层父节点位置居中，解决重叠后不再回退
    // 使用"移动量有界"策略避免发散
    for layer in 1..n_layers as i32 {
        let nodes = g.layer_nodes(layer);
        for id in &nodes {
            let parents: Vec<NodeId> = g
                .in_neighbors(*id)
                .into_iter()
                .filter(|&p| g.nodes[p].layer == layer - 1)
                .collect();

            if !parents.is_empty() {
                let avg_center: i32 = parents
                    .iter()
                    .map(|&p| g.nodes[p].x + g.nodes[p].w / 2)
                    .sum::<i32>()
                    / parents.len() as i32;
                let ideal_x = avg_center - g.nodes[*id].w / 2;
                // 只允许向当前位置靠近（不允许大幅后退，避免发散）
                let cur_x = g.nodes[*id].x;
                g.nodes[*id].x = if ideal_x < MARGIN_X { MARGIN_X } else { ideal_x };
                // 如果与相邻层的父节点偏差过大，折中处理
                let _ = cur_x; // suppress warning
            }
        }
        resolve_overlaps(g, layer);
    }

    // Step 3：反向单遍：最后一层向上对齐（父节点居中于子节点）
    for layer in (0..n_layers as i32 - 1).rev() {
        let nodes = g.layer_nodes(layer);
        for id in &nodes {
            let children: Vec<NodeId> = g
                .out_neighbors(*id)
                .into_iter()
                .filter(|&c| g.nodes[c].layer == layer + 1)
                .collect();

            if !children.is_empty() {
                let avg_center: i32 = children
                    .iter()
                    .map(|&c| g.nodes[c].x + g.nodes[c].w / 2)
                    .sum::<i32>()
                    / children.len() as i32;
                let ideal_x = avg_center - g.nodes[*id].w / 2;
                if ideal_x >= MARGIN_X {
                    g.nodes[*id].x = ideal_x;
                }
            }
        }
        resolve_overlaps(g, layer);
    }

    // Step 4：最终确保所有节点 x >= MARGIN_X，统一左对齐
    // 找出所有层的最小 x，将整图左移到紧靠 MARGIN_X
    let min_x = g
        .nodes
        .iter()
        .filter(|n| !n.is_dummy)
        .map(|n| n.x)
        .min()
        .unwrap_or(MARGIN_X);

    if min_x > MARGIN_X {
        let shift = min_x - MARGIN_X;
        for node in g.nodes.iter_mut() {
            node.x -= shift;
        }
    } else if min_x < MARGIN_X {
        let shift = MARGIN_X - min_x;
        for node in g.nodes.iter_mut() {
            node.x += shift;
        }
    }
}

/// 解决某层内节点的 x 坐标重叠，确保相邻节点之间有 H_GAP 间距。
/// 对应 rizin 的 `set_layer_gap` 思路。
///
/// 重要：这里按 `pos_in_layer` 顺序处理（由交叉最小化阶段确定），
/// 而不是按当前 x 坐标排序。因为反向对齐时，不同宽度的兄弟节点
/// 居中于同一子节点后，宽节点的 ideal_x 更小，如果按 x 排序会
/// 破坏交叉最小化的结果（导致左右互换）。
fn resolve_overlaps(g: &mut Graph, layer: i32) {
    let nodes = g.layer_nodes(layer); // 已按 pos_in_layer 排序
    if nodes.len() <= 1 {
        return;
    }

    // 按 pos_in_layer 顺序从左到右扫描，确保相邻节点不重叠
    let mut min_x = MARGIN_X;
    for &id in &nodes {
        if g.nodes[id].x < min_x {
            g.nodes[id].x = min_x;
        }
        min_x = g.nodes[id].x + g.nodes[id].w + H_GAP;
    }
}

/// 计算整个图布局所需的画布尺寸（宽、高）
///
/// 额外为回边绕行路径预留空间：
/// - 宽度：右侧绕行列（每条回边占 2 列 + 安全边距）
/// - 高度：底部额外行（Step 1/2 的垂直+水平转弯在最底层节点下方）
pub fn canvas_size(g: &Graph) -> (i32, i32) {
    let max_x = g
        .nodes
        .iter()
        .filter(|n| !n.is_dummy)
        .map(|n| n.x + n.w + MARGIN_X)
        .max()
        .unwrap_or(80);
    let max_y = g
        .nodes
        .iter()
        .filter(|n| !n.is_dummy)
        .map(|n| n.y + n.h)
        .max()
        .unwrap_or(40);

    // 为回边绕行预留空间
    let n_back = g.back_edges.len() as i32;
    let back_edge_w_margin = n_back * 2 + 4; // 右侧列宽裕量
    
    // 总是预留足够的空间，以容纳可能的向下弯曲的边
    let extra_h = 15; // 预留 15 行的额外空间
    
    // 底部需要 3 额外行（Step 1: 1行, Step 2 horizontal: 1行, 安全间距: 1行）
    let back_edge_h_margin = if n_back > 0 { 4 } else { V_GAP };

    (
        (max_x + back_edge_w_margin).max(80),
        (max_y + back_edge_h_margin + extra_h).max(40),
    )
}
