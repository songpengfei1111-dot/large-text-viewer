use crate::graph::{EdgeColor, Graph, Node, NodeId};
use crate::layout::canvas_size;

const CW: i32 = 8; // Cell width in pixels
const CH: i32 = 16; // Cell height in pixels
const MARGIN_TEXT_X: i32 = 2;
const TITLE_ROW_OFFSET: i32 = 1;
const BODY_ROW_OFFSET: i32 = 3;

fn edge_color_css(color: EdgeColor) -> &'static str {
    match color {
        EdgeColor::True => "#22c55e",         // green
        EdgeColor::False => "#ef4444",        // red
        EdgeColor::Unconditional => "#eab308",// yellow
    }
}

pub fn render_to_svg(g: &Graph) -> String {
    let (cw, ch) = canvas_size(g);
    let width_px = (cw + 2) * CW;
    let height_px = (ch + 2) * CH;

    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\" width=\"{}px\" height=\"{}px\">\n",
        width_px, height_px, width_px, height_px
    ));
    svg.push_str("  <style>\n");
    svg.push_str("    text { font-family: monospace; font-size: 14px; fill: #e5e7eb; }\n");
    svg.push_str("    .node-bg { fill: #1f2937; stroke-width: 1px; }\n");
    svg.push_str("    .node-border-blue { stroke: #3b82f6; }\n");
    svg.push_str("    .node-border-cyan { stroke: #06b6d4; }\n");
    svg.push_str("    .node-sep { stroke: #3b82f6; stroke-width: 1px; }\n");
    svg.push_str("    .title { fill: #eab308; }\n");
    svg.push_str("    .edge { fill: none; stroke-width: 1.5px; }\n");
    svg.push_str("    .arrow { fill: none; stroke-width: 1.5px; }\n"); // Could use marker-end
    svg.push_str("  </style>\n\n");

    // Add arrowhead markers
    svg.push_str("  <defs>\n");
    for (id, color) in [("true", "#22c55e"), ("false", "#ef4444"), ("uncond", "#eab308")] {
        svg.push_str(&format!(
            "    <marker id=\"arrow-{}\" viewBox=\"0 0 10 10\" refX=\"9\" refY=\"5\" markerWidth=\"6\" markerHeight=\"6\" orient=\"auto\">\n",
            id
        ));
        svg.push_str(&format!(
            "      <path d=\"M 0 0 L 10 5 L 0 10 z\" fill=\"{}\" />\n",
            color
        ));
        svg.push_str("    </marker>\n");
    }
    svg.push_str("  </defs>\n\n");

    svg.push_str("  <g id=\"edges\">\n");
    draw_edges_svg(&mut svg, g);
    draw_back_edges_svg(&mut svg, g);
    svg.push_str("  </g>\n\n");

    svg.push_str("  <g id=\"nodes\">\n");
    draw_nodes_svg(&mut svg, g);
    svg.push_str("  </g>\n\n");

    svg.push_str("</svg>\n");
    svg
}

fn draw_nodes_svg(svg: &mut String, g: &Graph) {
    for node in g.nodes.iter().filter(|n| !n.is_dummy) {
        draw_node_svg(svg, node, false);
    }
}

fn draw_node_svg(svg: &mut String, n: &Node, is_current: bool) {
    let x = n.x * CW;
    let y = n.y * CH;
    let w = n.w * CW;
    let h = n.h * CH;
    
    let border_class = if is_current { "node-border-cyan" } else { "node-border-blue" };
    
    svg.push_str(&format!(
        "    <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" class=\"node-bg {}\" rx=\"4\" />\n",
        x, y, w, h, border_class
    ));

    // Separator line
    let sep_y = y + 2 * CH;
    svg.push_str(&format!(
        "    <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" class=\"node-sep\" />\n",
        x, sep_y, x + w, sep_y
    ));

    // Title
    let title_x = x + MARGIN_TEXT_X * CW;
    let title_y = y + TITLE_ROW_OFFSET * CH + CH - 4; // Adjust baseline
    let max_w = (n.w - MARGIN_TEXT_X - 2) as usize;
    let title: String = n.title.chars().take(max_w).collect();
    // Escape XML
    let title = title.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;");
    svg.push_str(&format!(
        "    <text x=\"{}\" y=\"{}\" class=\"title\">{}</text>\n",
        title_x, title_y, title
    ));

    // Body
    let max_body_w = (n.w - MARGIN_TEXT_X * 2) as usize;
    let max_body_rows = (n.h - BODY_ROW_OFFSET - 1).max(0) as usize;

    for (row_idx, line) in n.body.lines().take(max_body_rows).enumerate() {
        let text_y = y + (BODY_ROW_OFFSET + row_idx as i32) * CH + CH - 4;
        let line_str: String = line.chars().take(max_body_w).collect();
        let line_str = line_str.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;");
        svg.push_str(&format!(
            "    <text x=\"{}\" y=\"{}\">{}</text>\n",
            title_x, text_y, line_str
        ));
    }
}

fn draw_edges_svg(svg: &mut String, g: &Graph) {
    for src_id in 0..g.node_count() {
        let src = &g.nodes[src_id];
        if src.is_dummy {
            continue;
        }

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
            let real_dst = find_real_dst(g, g.edges[ei].to);
            let waypoints = collect_waypoints(g, src_id, g.edges[ei].to, real_dst);

            let draw_color = if n_out > 2 {
                EdgeColor::Unconditional
            } else {
                match nth {
                    0 if n_out == 2 => EdgeColor::True,
                    1 if n_out == 2 => EdgeColor::False,
                    _ => edge_color,
                }
            };

            draw_edge_path_svg(svg, g, &waypoints, src_id, draw_color, nth, n_out);
        }
    }
}

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

fn draw_edge_path_svg(
    svg: &mut String,
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
    
    let (x1, y1) = waypoints[0];
    let (x2, y2) = *waypoints.last().unwrap();
    
    let pos = g.nodes[src_id].pos_in_layer;
    let bend_offset = pos + nth as i32 + 1;
    
    let mut effective_bend_y: i32;
    if y2 > y1 {
        let bend_y = y1 + bend_offset;
        effective_bend_y = if bend_y >= y2 - 1 { y1 + 1 } else { bend_y };
        
        let mut attempts = 0;
        while attempts < 100 {
            let mut blocked = false;
            for node in g.nodes.iter().filter(|n| !n.is_dummy) {
                if effective_bend_y >= node.y && effective_bend_y < node.y + node.h {
                    blocked = true;
                    break;
                }
            }
            if !blocked {
                break;
            }
            effective_bend_y += 1;
            if effective_bend_y >= y2 - 1 {
                effective_bend_y = y1 + 1;
                break;
            }
            attempts += 1;
        }
    } else {
        effective_bend_y = y1 + bend_offset.max(4);
    }
    
    let css_color = edge_color_css(color);
    let marker_id = match color {
        EdgeColor::True => "arrow-true",
        EdgeColor::False => "arrow-false",
        EdgeColor::Unconditional => "arrow-uncond",
    };

    let px1 = x1 * CW + CW / 2;
    let py1 = y1 * CH;
    let px2 = x2 * CW + CW / 2;
    let py2 = y2 * CH;
    let pbend_y = effective_bend_y * CH + CH / 2;

    svg.push_str(&format!(
        "    <polyline points=\"{},{} {},{} {},{} {},{}\" class=\"edge\" stroke=\"{}\" marker-end=\"url(#{})\" />\n",
        px1, py1, px1, pbend_y, px2, pbend_y, px2, py2, css_color, marker_id
    ));
}

fn draw_back_edges_svg(svg: &mut String, g: &Graph) {
    for (idx, &(from_id, to_id, color)) in g.back_edges.iter().enumerate() {
        let src = &g.nodes[from_id];
        let dst = &g.nodes[to_id];

        let ax = src.bottom_center_x();
        let ay = src.bottom_y();
        let bx = dst.top_center_x();
        let by = dst.top_y() - 1;

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

        let xbendpoint = if right_len < left_len {
            max_x + 1 + (idx as i32) * 2
        } else {
            (min_x - 2 - (idx as i32) * 2).max(0)
        };

        let ybp1: i32 = 0;
        let ybp2: i32 = 0;

        let css_color = edge_color_css(color);
        let marker_id = match color {
            EdgeColor::True => "arrow-true",
            EdgeColor::False => "arrow-false",
            EdgeColor::Unconditional => "arrow-uncond",
        };

        let p_ax = ax * CW + CW / 2;
        let p_ay = ay * CH;
        
        let p_h2_y = (ay + ybp1 + 2) * CH + CH / 2;
        let p_xbend = xbendpoint * CW + CW / 2;
        
        let p_h4_y = (by - ybp2) * CH + CH / 2;
        let p_bx = bx * CW + CW / 2;
        let p_by = (by + 1) * CH; // point into the target node

        svg.push_str(&format!(
            "    <polyline points=\"{},{} {},{} {},{} {},{} {},{} {},{}\" class=\"edge\" stroke=\"{}\" marker-end=\"url(#{})\" fill=\"none\" />\n",
            p_ax, p_ay,
            p_ax, p_h2_y,
            p_xbend, p_h2_y,
            p_xbend, p_h4_y,
            p_bx, p_h4_y,
            p_bx, p_by,
            css_color, marker_id
        ));
    }
}
