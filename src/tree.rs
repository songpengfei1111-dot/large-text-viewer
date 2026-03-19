// tree.rs - 通用树结构模块
// 提供与业务逻辑无关的基本树操作

use std::fmt;

/// 通用树节点 trait，定义树节点的基本操作
pub trait TreeNode: Clone + fmt::Debug {
    /// 获取子节点的可变引用
    fn children_mut(&mut self) -> &mut Vec<Self>;
    
    /// 获取子节点的不可变引用
    fn children(&self) -> &[Self];
    
    /// 获取节点深度
    fn depth(&self) -> usize;
    
    /// 添加单个子节点
    fn add_child(&mut self, child: Self) {
        self.children_mut().push(child);
    }
    
    /// 添加多个子节点
    fn add_children(&mut self, children: impl IntoIterator<Item = Self>) {
        self.children_mut().extend(children);
    }
}

/// 通用树结构 trait
pub trait Tree<T: TreeNode> {
    /// 获取根节点
    fn root(&self) -> Option<&T>;
    
    /// 获取根节点的可变引用
    fn root_mut(&mut self) -> Option<&mut T>;
    
    /// 设置根节点
    fn set_root(&mut self, root: T);
    
    /// 计算树的最大深度
    fn max_depth(&self) -> usize {
        self.root()
            .map(|root| Self::calculate_max_depth(root))
            .unwrap_or(0)
    }
    
    /// 计算节点的最大深度（内部方法）
    fn calculate_max_depth(node: &T) -> usize {
        if node.children().is_empty() {
            node.depth()
        } else {
            node.children()
                .iter()
                .map(Self::calculate_max_depth)
                .max()
                .unwrap_or(node.depth())
        }
    }
    
    /// 统计树中的节点总数
    fn count_nodes(&self) -> usize {
        self.root()
            .map(|root| 1 + Self::count_children_nodes(root))
            .unwrap_or(0)
    }
    
    /// 统计子节点数量（内部方法）
    fn count_children_nodes(node: &T) -> usize {
        node.children()
            .iter()
            .map(|child| 1 + Self::count_children_nodes(child))
            .sum()
    }
    
    /// 前序遍历树
    fn pre_order_traverse<F: FnMut(&T)>(&self, mut f: F) {
        if let Some(root) = self.root() {
            Self::pre_order_traverse_node(root, &mut f);
        }
    }
    
    /// 前序遍历节点（内部方法）
    fn pre_order_traverse_node<F: FnMut(&T)>(node: &T, f: &mut F) {
        f(node);
        for child in node.children() {
            Self::pre_order_traverse_node(child, f);
        }
    }
    
    /// 后序遍历树
    fn post_order_traverse<F: FnMut(&T)>(&self, mut f: F) {
        if let Some(root) = self.root() {
            Self::post_order_traverse_node(root, &mut f);
        }
    }
    
    /// 后序遍历节点（内部方法）
    fn post_order_traverse_node<F: FnMut(&T)>(node: &T, f: &mut F) {
        for child in node.children() {
            Self::post_order_traverse_node(child, f);
        }
        f(node);
    }
}

/// 基本的树节点实现（可作为示例或基础）
#[derive(Debug, Clone)]
pub struct BasicTreeNode {
    pub id: usize,
    pub depth: usize,
    pub children: Vec<BasicTreeNode>,
}

impl BasicTreeNode {
    pub fn new(id: usize, depth: usize) -> Self {
        Self {
            id,
            depth,
            children: Vec::new(),
        }
    }
}

impl TreeNode for BasicTreeNode {
    fn children_mut(&mut self) -> &mut Vec<Self> {
        &mut self.children
    }
    
    fn children(&self) -> &[Self] {
        &self.children
    }
    
    fn depth(&self) -> usize {
        self.depth
    }
}

/// 基本的树实现
#[derive(Debug, Clone)]
pub struct BasicTree {
    root: Option<BasicTreeNode>,
    next_id: usize,
}

impl BasicTree {
    pub fn new() -> Self {
        Self {
            root: None,
            next_id: 0,
        }
    }
    
    pub fn next_id(&mut self) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

impl Tree<BasicTreeNode> for BasicTree {
    fn root(&self) -> Option<&BasicTreeNode> {
        self.root.as_ref()
    }
    
    fn root_mut(&mut self) -> Option<&mut BasicTreeNode> {
        self.root.as_mut()
    }
    
    fn set_root(&mut self, root: BasicTreeNode) {
        self.root = Some(root);
    }
}
