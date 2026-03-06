/// 渲染模块：将已布局的图绘制到 Canvas 上
///
/// 对应 rizin agraph.c 中的：
/// - `agraph_print_nodes(g)` —— 遍历节点并调用 `normal_RzANode_print`
/// - `agraph_print_edges(g)` —— 绘制节点间的连线
/// - `agraph_print(g, ...)` ——  主渲染函数

use crate::canvas::{Canvas, color};
use crate::graph::{EdgeColor, Graph, Node, NodeId};
use crate::layout::canvas_size;

// ── 渲染常量（对应 agraph.c 宏定义）─────────────────────────────
/// 节点内文字左边距（MARGIN_TEXT_X = 2）
const MARGIN_TEXT_X: i32 = 2;
/// 节点内文字上边距（MARGIN_TEXT_Y = 2，已含标题行 + 分隔线）
const TITLE_ROW_OFFSET: i32 = 1; // 标题在边框内第 1 行
const BODY_ROW_OFFSET: i32 = 3;  // 正文在边框内第 3 行（0=顶边，1=标题，2=分隔线，3=首行正文）

// ── 边颜色 ANSI 映射 ────────────────────────────────────────────
fn edge_attr(color: EdgeColor) -> &'static str {
    match color {
        EdgeColor::True => color::BRIGHT_GREEN,
        EdgeColor::False => color::BRIGHT_RED,
        EdgeColor::Unconditional => color::YELLOW,
    }
}

// ── 主渲染函数 ──────────────────────────────────────────────────

/// 把已经完成布局的图 `g` 渲染到一个新建的 Canvas 并返回。
///
/// 绘制顺序严格匹配 rizin 的 `agraph_print(g, ...)`：
///   agraph_print_edges(g);   // 先画所有边（含前向边 + 回边）
///   agraph_print_nodes(g);   // 再画节点（边框 + 内容覆盖穿过的边线）
///
/// 节点绘制时先填充内部为空格，确保穿过节点区域的边线被完全清除。
pub fn render(g: &Graph) -> Canvas {
    let (cw, ch) = canvas_size(g);
    let mut canvas = Canvas::new(cw, ch);
    canvas.use_color = true;

    // 步骤 1：绘制所有边（前向边 + 回边），对应 agraph_print_edges(g)
    draw_edges(&mut canvas, g);
    draw_back_edges(&mut canvas, g);

    // 步骤 2：绘制节点（覆盖穿过节点的边线），对应 agraph_print_nodes(g)
    draw_nodes(&mut canvas, g);

    canvas
}

/// 直接渲染到标准输出（便捷函数）
pub fn render_to_stdout(g: &Graph) {
    let canvas = render(g);
    print!("{}", canvas.to_string_output());
}

// ── 节点绘制 ─────────────────────────────────────────────────────

/// 绘制所有非哑节点，对应 `agraph_print_nodes(g)`
fn draw_nodes(canvas: &mut Canvas, g: &Graph) {
    for node in g.nodes.iter().filter(|n| !n.is_dummy) {
        draw_node(canvas, node, false);
    }
}

/// 绘制单个节点，对应 `normal_RzANode_print(g, n, cur)`
///
/// 节点结构示意：
/// ```text
/// ┌────────────────────┐  ← y
/// │ <title>            │  ← y+1
/// ├────────────────────┤  ← y+2  （分隔线）
/// │ line 1 of body     │  ← y+3
/// │ line 2 of body     │  ← y+4
/// └────────────────────┘  ← y+h-1
/// ```
fn draw_node(canvas: &mut Canvas, n: &Node, is_current: bool) {
    // 选择节点边框颜色：当前节点用青色，其余用蓝色
    let box_attr = if is_current {
        Some(color::CYAN)
    } else {
        Some(color::BLUE)
    };

    // 1. 绘制矩形边框（Unicode 盒子字符）
    canvas.draw_box(n.x, n.y, n.w, n.h, box_attr);

    // 1.5 填充节点内部为空格，清除穿过节点区域的边线
    // 对应 rizin 中 rz_cons_canvas_write 逐字符覆盖 + rz_cons_canvas_box 画边框的效果
    if n.w > 2 && n.h > 2 {
        canvas.current_attr = None;
        canvas.fill(n.x + 1, n.y + 1, n.w - 2, n.h - 2, ' ');
    }

    // 2. 绘制标题（第 1 行，黄色）
    canvas.current_attr = Some(color::YELLOW);
    if canvas.goto(n.x + MARGIN_TEXT_X, n.y + TITLE_ROW_OFFSET) {
        let max_w = (n.w - MARGIN_TEXT_X - 2) as usize; // 两侧各留 1 格
        let title: String = n.title.chars().take(max_w).collect();
        canvas.write(&title);
    }

    // 3. 绘制标题与正文之间的分隔线（├────┤）
    canvas.draw_separator(n.x, n.y + 2, n.w, box_attr);

    // 4. 绘制正文（从 y+BODY_ROW_OFFSET 开始，逐行写入）
    canvas.current_attr = None;
    let max_body_w = (n.w - MARGIN_TEXT_X * 2) as usize;
    let max_body_rows = (n.h - BODY_ROW_OFFSET - 1).max(0) as usize; // 减去底边框

    for (row_idx, line) in n.body.lines().take(max_body_rows).enumerate() {
        let row_y = n.y + BODY_ROW_OFFSET + row_idx as i32;
        if canvas.goto(n.x + MARGIN_TEXT_X, row_y) {
            // 截断过长的行（对应 rz_str_ansi_crop）
            let line_str: String = line.chars().take(max_body_w).collect();
            canvas.write(&line_str);
        }
    }

    canvas.current_attr = None;
}

// ── 边绘制 ───────────────────────────────────────────────────────

/// 绘制图中所有边（跳过哑节点相关边的中间段，统一走完整路径）
/// 对应 `agraph_print_edges(g)` 的核心逻辑
fn draw_edges(canvas: &mut Canvas, g: &Graph) {
    // 为每个非哑节点，找出它的"真实后继"（穿越哑节点链后的目标），绘制完整边路径
    for src_id in 0..g.node_count() {
        let src = &g.nodes[src_id];
        if src.is_dummy {
            continue;
        }

        // 按出边顺序处理（跳过已反转的回边——回边由 draw_back_edges 单独绘制）
        let out_edges: Vec<usize> = g.adj_out[src_id].clone();
        let n_out = out_edges
            .iter()
            .filter(|&&ei| !g.edges[ei].dead && !g.edges[ei].reversed)
            .count();

        for (nth, &ei) in out_edges
            .iter()
            .filter(|&&ei| !g.edges[ei].dead && !g.edges[ei].reversed)
            .enumerate()
        {
            let edge_color = g.edges[ei].color;

            // 沿着哑节点链找到真正的目标节点
            let real_dst = find_real_dst(g, g.edges[ei].to);

            // 收集路径上的全部 x 坐标点（含哑节点）
            let waypoints = collect_waypoints(g, src_id, g.edges[ei].to, real_dst);

            // 确定边颜色（出度 > 2 时统一用无条件颜色；≤2 时按 true/false 区分）
            let draw_color = if n_out > 2 {
                EdgeColor::Unconditional
            } else {
                match nth {
                    0 if n_out == 2 => EdgeColor::True,
                    1 if n_out == 2 => EdgeColor::False,
                    _ => edge_color,
                }
            };

            draw_edge_path(canvas, g, &waypoints, src_id, draw_color, nth, n_out);
        }
    }
}

/// 沿哑节点链找到真实目标节点
fn find_real_dst(g: &Graph, start: NodeId) -> NodeId {
    let mut cur = start;
    loop {
        if !g.nodes[cur].is_dummy {
            return cur;
        }
        let nexts = g.out_neighbors(cur);
        if nexts.is_empty() {
            return cur;
        }
        cur = nexts[0];
    }
}

/// 收集从 src 到 real_dst 路径上所有节点的 x 坐标（包含哑节点段）
fn collect_waypoints(g: &Graph, src: NodeId, first_hop: NodeId, real_dst: NodeId) -> Vec<(i32, i32)> {
    let src_node = &g.nodes[src];
    let mut points = vec![(src_node.bottom_center_x(), src_node.bottom_y())];

    let mut cur = first_hop;
    while cur != real_dst {
        if g.nodes[cur].is_dummy {
            let dn = &g.nodes[cur];
            points.push((dn.x, dn.y));
        }
        let nexts = g.out_neighbors(cur);
        if nexts.is_empty() {
            break;
        }
        cur = nexts[0];
    }

    let dst_node = &g.nodes[real_dst];
    points.push((dst_node.top_center_x(), dst_node.top_y()));
    points
}

/// 绘制一条完整的边路径（折线）
///
/// bend_y 的计算是避免同层多条边在同一水平行重叠的关键。
/// 对应 rizin 的 `tm->edgectr` / `bendpoint` 逻辑。
///
/// 修复前的问题：两条来自同层不同节点的边（如 0x1010→exit 和 0x1030→exit）
/// 都用 `bend_y = y1 + nth + 1`，y1 相同、nth 相同，导致水平段完全重叠，
/// 宽边的 `─` 会覆盖窄边的转角字符 `┘`。
///
/// 修复方案：bend_y 还要加上源节点在本层的位置偏移 `pos_in_layer`，
/// 确保不同位置的节点用不同的水平行。
fn draw_edge_path(
    canvas: &mut Canvas,
    g: &Graph,
    waypoints: &[(i32, i32)],
    src_id: NodeId,
    color: EdgeColor,
    nth: usize,
    _n_out: usize,
) {
    if waypoints.len() < 2 {
        return;
    }
    let attr = Some(edge_attr(color));

    let (x1, y1) = waypoints[0];
    let (x2, y2) = *waypoints.last().unwrap();

    // bend_y = 起点下方 (pos_in_layer + nth + 1) 行
    // pos_in_layer 保证同层不同节点使用不同行，nth 保证同节点多条出边也不重叠
    let pos = g.nodes[src_id].pos_in_layer;
    let bend_offset = pos + nth as i32 + 1;
    let bend_y = y1 + bend_offset;

    // 如果 bend_y 过大（超过目标顶部），折中处理
    let effective_bend_y = if bend_y >= y2 - 1 {
        (y1 + y2) / 2
    } else {
        bend_y
    };

    canvas.draw_edge(x1, y1, x2, y2, effective_bend_y, attr);
}

/// 绘制所有回边（从下往上的弧线）。
///
/// 完全对应 rizin 的 `rz_cons_canvas_line_back_edge(c, ax, ay, bx, by, ..., ybendpoint1, xbendpoint, ybendpoint2, isvert=true)`
///
/// 5 步路由策略：从源节点底部中心向下延伸 → 水平转到绕行列 → 绕行列向上爬升
/// → 水平转到目标节点顶部中心 → 向下进入目标节点。
///
/// 示意图（右绕行）：
/// ```text
///         bx
///    ┌────────┐      ← Step 4: ┌...┐  (y = by - ybp2)
///    │        │
///    │   ┌────────────┐
///    │   │  target    │    ← dst.y
///    │   └────────────┘
///    │        │
///    │        ▼   (forward edges to children...)
///    │        ...
///    │   ┌────────────┐
///    │   │  source    │    ← src.y
///    │   └────────────┘
///    │        │            ← Step 1: │ (y = ay+1)
///    └────────┘            ← Step 2: └...┘ (y = ay + ybp1 + 2)
///         ax
/// ```
fn draw_back_edges(canvas: &mut Canvas, g: &Graph) {
    // 找出涉及层范围内所有节点的最左/最右边界，决定绕行方向
    for (idx, &(from_id, to_id, color)) in g.back_edges.iter().enumerate() {
        let src = &g.nodes[from_id];
        let dst = &g.nodes[to_id];
        let attr = Some(edge_attr(color));

        // ── 源/目标坐标（对应 rizin: ax, ay, bx, by）────────────────────
        let ax = src.bottom_center_x();
        let ay = src.bottom_y();         // = src.y + src.h（底部边框下方一行）
        let bx = dst.top_center_x();
        let by = dst.top_y() - 1;        // = dst.y - 1（顶部边框上方一行）

        // ── 绕行方向决策（对应 rizin backedge_info 的 leftlen/rightlen）──
        let dst_layer = dst.layer;
        let src_layer = src.layer;
        let mut min_x = i32::MAX;
        let mut max_x = i32::MIN;
        for node in g.nodes.iter().filter(|n| !n.is_dummy) {
            if node.layer >= dst_layer && node.layer <= src_layer {
                min_x = min_x.min(node.x);
                max_x = max_x.max(node.x + node.w);
            }
        }
        if min_x == i32::MAX {
            min_x = ax.min(bx);
            max_x = ax.max(bx);
        }

        let left_len = (ax - min_x) + (bx - min_x);
        let right_len = (max_x - ax) + (max_x - bx);

        // 绕行列：右侧或左侧，每条回边额外偏移 2 列避免重叠
        let xbendpoint = if right_len < left_len {
            max_x + 1 + (idx as i32) * 2
        } else {
            (min_x - 2 - (idx as i32) * 2).max(0)
        };

        // 弯曲偏移（简化版本，对应 rizin 的 ybendpoint1/ybendpoint2）
        let ybp1: i32 = 0;
        let ybp2: i32 = 0;

        canvas.current_attr = attr;

        // ── Step 1：从源底部中心向下延伸 ────────────────────────────────
        // 对应 apply_line_style(c, x, y, ...) 在 (ax, ay) 画起始 │
        // + draw_vertical_line(c, x, y+1, ybendpoint1+1)
        for y in ay..=(ay + 1 + ybp1) {
            if canvas.goto(ax, y) {
                canvas.put_merge('│');
            }
        }

        // ── Step 2：水平转弯 └──────┘（REV_APEX_APEX）─────────────────
        // 对应 draw_horizontal_line(c, min_x1, y+ybp1+2, w1, REV_APEX_APEX)
        // 角字符：└(CORNER_BL: ↑+→)  ┘(CORNER_BR: ↑+←)
        let h2_y = ay + ybp1 + 2;
        let h2_left = ax.min(xbendpoint);
        let h2_right = ax.max(xbendpoint);
        if h2_right > h2_left {
            if canvas.goto(h2_left, h2_y) {
                canvas.put_merge('└');
            }
            for x in (h2_left + 1)..h2_right {
                if canvas.goto(x, h2_y) {
                    canvas.put_merge('─');
                }
            }
            if canvas.goto(h2_right, h2_y) {
                canvas.put_merge('┘');
            }
        }

        // ── Step 3：绕行列上的垂直线段 ────────────────────────────────
        // 对应 draw_vertical_line(c, xbendpoint, y2-ybp2+1, diff_y-1)
        // 连接 Step 2 的 ┘/└ 和 Step 4 的 ┐/┌
        let side_top = by - ybp2 + 1;
        let side_bot = ay + ybp1 + 1;
        for y in side_top..=side_bot {
            if canvas.goto(xbendpoint, y) {
                canvas.put_merge('│');
            }
        }

        // ── Step 4：水平转弯 ┌──────┐（DOT_DOT）──────────────────────
        // 对应 draw_horizontal_line(c, min_x2, y2-ybp2, w2, DOT_DOT)
        // 角字符：┌(CORNER_TL: ↓+→)  ┐(CORNER_TR: ↓+←)
        let h4_y = by - ybp2;
        let h4_left = bx.min(xbendpoint);
        let h4_right = bx.max(xbendpoint);
        if h4_right > h4_left {
            if canvas.goto(h4_left, h4_y) {
                canvas.put_merge('┌');
            }
            for x in (h4_left + 1)..h4_right {
                if canvas.goto(x, h4_y) {
                    canvas.put_merge('─');
                }
            }
            if canvas.goto(h4_right, h4_y) {
                canvas.put_merge('┐');
            }
        }

        // ── Step 5：从顶部转弯处向下进入目标节点 ──────────────────────
        // 对应 draw_vertical_line(c, x2, y2-ybp2+1, ybp2+1)
        // 回边在节点之前绘制，Step 5 的 │ 会被节点边框覆盖，
        // 但 Step 4 的水平线在节点上方仍然可见，标识回边入口。
        for y in (by - ybp2 + 1)..=(by + 1) {
            if canvas.goto(bx, y) {
                canvas.put_merge('│');
            }
        }

        canvas.current_attr = None;
    }
}
