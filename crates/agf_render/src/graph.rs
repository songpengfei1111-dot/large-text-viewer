/// 节点 ID（索引到 Graph::nodes 数组）
pub type NodeId = usize;

/// 边的颜色/类型，对应 rizin 的 LINE_TRUE / LINE_FALSE / LINE_UNCJMP
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeColor {
    /// 无条件跳转（蓝色/黄色）
    Unconditional,
    /// 条件跳转为真（绿色）
    True,
    /// 条件跳转为假（红色）
    False,
}

/// 图中的一条有向边，对应 rizin 的 `RzGraphEdge`
#[derive(Debug, Clone)]
pub struct Edge {
    pub from: NodeId,
    pub to: NodeId,
    /// 边的颜色/类型
    pub color: EdgeColor,
    /// 是否是被反转的回边（用于环路处理）
    pub reversed: bool,
    /// 是否已被"删除"（插入哑节点后原始长边标记为无效）
    pub dead: bool,
}

/// 图中的一个节点，对应 rizin 的 `RzANode`
#[derive(Debug, Clone)]
pub struct Node {
    /// 节点索引
    pub id: NodeId,
    /// 节点标题（通常是地址，如 "0x1000"）
    pub title: String,
    /// 节点正文（反汇编文本，多行）
    pub body: String,

    // ─── 布局信息（由 layout.rs 填充）───────────────────
    /// 节点左上角 x 坐标（画布坐标）
    pub x: i32,
    /// 节点左上角 y 坐标（画布坐标）
    pub y: i32,
    /// 节点宽度（含边框）
    pub w: i32,
    /// 节点高度（含边框）
    pub h: i32,
    /// 所属层编号（0 = 顶层）
    pub layer: i32,
    /// 在本层中的排列位置（0-indexed）
    pub pos_in_layer: i32,

    // ─── 哑节点信息（用于拆分跨层边）────────────────────
    /// true = 这是插入的哑节点，不渲染
    pub is_dummy: bool,
    /// 哑节点所属的原始边（from, to）
    pub dummy_edge: Option<(NodeId, NodeId)>,
}

impl Node {
    /// 创建普通节点，自动根据内容计算初始尺寸
    pub fn new(id: NodeId, title: &str, body: &str) -> Self {
        // 计算内容的最大宽度
        let body_max_w = body
            .lines()
            .map(|l| l.chars().count())
            .max()
            .unwrap_or(0) as i32;
        let title_w = title.chars().count() as i32 + 2; // 左右各留 1 格空白

        // 内容宽度 + 边框(2) + 内边距(2)
        let content_w = body_max_w.max(title_w);
        let w = (content_w + 4).max(12); // 最小宽度 12

        // 正文行数 + 标题行(1) + 分隔线(1) + 上边框(1) + 下边框(1) = +4
        let body_lines = body.lines().count() as i32;
        let h = (body_lines + 4).max(4); // 最小高度 4

        Node {
            id,
            title: title.to_string(),
            body: body.to_string(),
            x: 0,
            y: 0,
            w,
            h,
            layer: -1,
            pos_in_layer: 0,
            is_dummy: false,
            dummy_edge: None,
        }
    }

    /// 创建哑节点（用于拆分跨多层的长边）
    pub fn new_dummy(id: NodeId, layer: i32, orig_from: NodeId, orig_to: NodeId) -> Self {
        Node {
            id,
            title: String::new(),
            body: String::new(),
            x: 0,
            y: 0,
            w: 1,
            h: 1,
            layer,
            pos_in_layer: 0,
            is_dummy: true,
            dummy_edge: Some((orig_from, orig_to)),
        }
    }

    /// 返回节点底部中心的 x 坐标（边的起始点）
    #[inline]
    pub fn bottom_center_x(&self) -> i32 {
        self.x + self.w / 2
    }

    /// 返回节点底部的 y 坐标（边的起始点，在边框外面一格）
    #[inline]
    pub fn bottom_y(&self) -> i32 {
        self.y + self.h
    }

    /// 返回节点顶部中心的 x 坐标（边的终点）
    #[inline]
    pub fn top_center_x(&self) -> i32 {
        self.x + self.w / 2
    }

    /// 返回节点顶部的 y 坐标（边的终点）
    #[inline]
    pub fn top_y(&self) -> i32 {
        self.y
    }
}

/// 有向图，对应 rizin 的 `RzAGraph` 中的图结构部分
///
/// - `nodes`: 所有节点（包括哑节点）
/// - `edges`: 所有边（包括已反转的回边）
/// - `adj_out[i]`: 从节点 i 出发的边的索引列表
/// - `adj_in[i]`:  到达节点 i 的边的索引列表
/// - `back_edges`: 环路消除前记录的原始回边 (from, to, color)
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    /// adj_out[node_id] = Vec<edge_idx>
    pub adj_out: Vec<Vec<usize>>,
    /// adj_in[node_id] = Vec<edge_idx>
    pub adj_in: Vec<Vec<usize>>,
    /// 原始回边列表（由 remove_cycles 填充），对应 rizin 的 g->back_edges
    /// 每项 = (原始 from, 原始 to, 颜色)
    pub back_edges: Vec<(NodeId, NodeId, EdgeColor)>,
}

impl Graph {
    pub fn new() -> Self {
        Graph {
            nodes: Vec::new(),
            edges: Vec::new(),
            adj_out: Vec::new(),
            adj_in: Vec::new(),
            back_edges: Vec::new(),
        }
    }

    /// 添加普通节点，返回其 NodeId
    pub fn add_node(&mut self, title: &str, body: &str) -> NodeId {
        let id = self.nodes.len();
        self.nodes.push(Node::new(id, title, body));
        self.adj_out.push(Vec::new());
        self.adj_in.push(Vec::new());
        id
    }

    /// 添加哑节点（内部使用）
    pub(crate) fn add_dummy_node(&mut self, layer: i32, orig_from: NodeId, orig_to: NodeId) -> NodeId {
        let id = self.nodes.len();
        self.nodes.push(Node::new_dummy(id, layer, orig_from, orig_to));
        self.adj_out.push(Vec::new());
        self.adj_in.push(Vec::new());
        id
    }

    /// 添加带颜色的有向边，返回边的索引
    pub fn add_edge(&mut self, from: NodeId, to: NodeId, color: EdgeColor) -> usize {
        let idx = self.edges.len();
        self.edges.push(Edge {
            from,
            to,
            color,
            reversed: false,
            dead: false,
        });
        self.adj_out[from].push(idx);
        self.adj_in[to].push(idx);
        idx
    }

    /// 添加无条件边（便捷接口）
    pub fn add_edge_uncond(&mut self, from: NodeId, to: NodeId) -> usize {
        self.add_edge(from, to, EdgeColor::Unconditional)
    }

    /// 修改某条边的起点（用于哑节点插入时重连）
    pub(crate) fn redirect_edge_from(&mut self, edge_idx: usize, new_from: NodeId) {
        let old_from = self.edges[edge_idx].from;
        self.adj_out[old_from].retain(|&e| e != edge_idx);
        self.edges[edge_idx].from = new_from;
        self.adj_out[new_from].push(edge_idx);
    }

    /// 修改某条边的终点（用于哑节点插入时重连）
    pub(crate) fn redirect_edge_to(&mut self, edge_idx: usize, new_to: NodeId) {
        let old_to = self.edges[edge_idx].to;
        self.adj_in[old_to].retain(|&e| e != edge_idx);
        self.edges[edge_idx].to = new_to;
        self.adj_in[new_to].push(edge_idx);
    }

    /// 获取节点 id 的所有出边目标节点 ID
    pub fn out_neighbors(&self, id: NodeId) -> Vec<NodeId> {
        self.adj_out[id]
            .iter()
            .filter(|&&ei| !self.edges[ei].dead)
            .map(|&ei| self.edges[ei].to)
            .collect()
    }

    /// 获取节点 id 的所有入边源节点 ID
    pub fn in_neighbors(&self, id: NodeId) -> Vec<NodeId> {
        self.adj_in[id]
            .iter()
            .filter(|&&ei| !self.edges[ei].dead)
            .map(|&ei| self.edges[ei].from)
            .collect()
    }

    /// 获取某条从 from 到 to 的边颜色（第一条匹配）
    pub fn edge_color(&self, from: NodeId, to: NodeId) -> EdgeColor {
        for &ei in &self.adj_out[from] {
            if self.edges[ei].to == to && !self.edges[ei].dead {
                return self.edges[ei].color;
            }
        }
        EdgeColor::Unconditional
    }

    /// 节点总数
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// 层数（仅统计已分配层的节点中的最大层号 + 1）
    pub fn layer_count(&self) -> usize {
        self.nodes
            .iter()
            .filter(|n| n.layer >= 0)
            .map(|n| n.layer + 1)
            .max()
            .unwrap_or(0) as usize
    }

    /// 返回某一层的所有节点 ID（按 pos_in_layer 排序）
    pub fn layer_nodes(&self, layer: i32) -> Vec<NodeId> {
        let mut ids: Vec<NodeId> = self
            .nodes
            .iter()
            .filter(|n| n.layer == layer)
            .map(|n| n.id)
            .collect();
        ids.sort_by_key(|&id| self.nodes[id].pos_in_layer);
        ids
    }
}

impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}
