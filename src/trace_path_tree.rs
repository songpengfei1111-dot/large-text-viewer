use agf_render::{Graph, EdgeColor, layout, render_to_stdout};
use crate::taint_engine::{TracePath, TraceType};
use std::fmt;

const MAX_LINE_LENGTH: usize = 60;

#[derive(Debug, Clone)]
pub struct TreeNode<T> {
    value: T,
    left: Option<Box<TreeNode<T>>>,
    right: Option<Box<TreeNode<T>>>,
}

#[derive(Debug)]
pub struct BinaryTree<T> {
    root: Option<Box<TreeNode<T>>>,
}

impl<T> TreeNode<T> {
    fn new(value: T) -> Self {
        TreeNode {
            value,
            left: None,
            right: None,
        }
    }
}

impl<T: PartialEq + Clone + std::fmt::Debug> BinaryTree<T> {
    pub fn new() -> Self {
        BinaryTree { root: None }
    }

    pub fn add_root(&mut self, value: T) {
        self.root = Some(Box::new(TreeNode::new(value)));
    }

    pub fn add_child(&mut self, parent_value: T, child_value: T, is_left: bool) -> bool {
        self.root.as_mut().map_or(false, |root| {
            Self::add_child_recursive(root, &parent_value, child_value, is_left)
        })
    }

    fn add_child_recursive(
        node: &mut Box<TreeNode<T>>,
        parent_value: &T,
        child_value: T,
        is_left: bool,
    ) -> bool {
        if node.value == *parent_value {
            let child = Box::new(TreeNode::new(child_value));
            let target = if is_left { &mut node.left } else { &mut node.right };
            if target.is_none() {
                *target = Some(child);
                return true;
            }
            return false;
        }

        node.left.as_mut().map_or(false, |left| {
            Self::add_child_recursive(left, parent_value, child_value.clone(), is_left)
        }) || node.right.as_mut().map_or(false, |right| {
            Self::add_child_recursive(right, parent_value, child_value, is_left)
        })
    }

    pub fn add_left(&mut self, parent_value: T, child_value: T) -> bool {
        self.add_child(parent_value, child_value, true)
    }

    pub fn add_right(&mut self, parent_value: T, child_value: T) -> bool {
        self.add_child(parent_value, child_value, false)
    }

    pub fn to_graph(&self) -> Graph {
        let mut graph = Graph::new();
        if let Some(root) = &self.root {
            Self::add_tree_to_graph(root, &mut graph);
        }
        graph
    }

    fn add_tree_to_graph(tree_node: &Box<TreeNode<T>>, graph: &mut Graph) -> usize {
        let value_str = format!("{:?}", tree_node.value);
        let lines: Vec<&str> = value_str.lines().collect();
        let title = if lines.is_empty() { "" } else { lines[0] };
        let body = if lines.len() > 1 { lines[1..].join("\n") } else { String::new() };
        let node_id = graph.add_node(title, &body);

        if let Some(left) = &tree_node.left {
            let left_id = Self::add_tree_to_graph(left, graph);
            graph.add_edge(node_id, left_id, EdgeColor::True);
        }

        if let Some(right) = &tree_node.right {
            let right_id = Self::add_tree_to_graph(right, graph);
            graph.add_edge(node_id, right_id, EdgeColor::False);
        }

        node_id
    }

    pub fn render(&self) {
        let mut graph = self.to_graph();
        layout(&mut graph);
        render_to_stdout(&graph);
    }
}

impl<T> Default for BinaryTree<T> {
    fn default() -> Self {
        BinaryTree { root: None }
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct TraceNode {
    pub id: usize,
    pub display_text: String,
}

impl fmt::Display for TraceNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_text)
    }
}

impl fmt::Debug for TraceNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
    }
}

pub struct TracePathTree {
    tree: BinaryTree<TraceNode>,
    next_id: usize,
}

impl TracePathTree {
    pub fn new() -> Self {
        TracePathTree {
            tree: BinaryTree::new(),
            next_id: 0,
        }
    }

    pub fn from_trace_path(trace: &TracePath) -> Self {
        let mut tree = TracePathTree::new();
        tree.build_from_trace(trace);
        tree
    }

    fn next_id(&mut self) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn build_from_trace(&mut self, trace: &TracePath) {
        let mut merged_traces = Vec::new();
        Self::merge_linear_chain(trace, &mut merged_traces);
        
        let root_node = self.create_merged_node(&merged_traces);
        self.tree.add_root(root_node.clone());
        
        if let Some(last_trace) = merged_traces.last() {
            self.build_children_recursive(&root_node, &last_trace.sources);
        }
    }

    fn merge_linear_chain(trace: &TracePath, result: &mut Vec<TracePath>) {
        result.push(trace.clone());
        
        if trace.sources.len() == 1 {
            let child = &trace.sources[0];
            if child.sources.len() <= 1 {
                Self::merge_linear_chain(child, result);
                return;
            }
        }
    }

    fn build_children_recursive(&mut self, parent: &TraceNode, children: &[TracePath]) {
        for (idx, child) in children.iter().enumerate() {
            let mut merged_traces = Vec::new();
            Self::merge_linear_chain(child, &mut merged_traces);
            
            let child_node = self.create_merged_node(&merged_traces);
            let is_left = idx % 2 == 0;
            self.tree.add_child(parent.clone(), child_node.clone(), is_left);
            
            if let Some(last_trace) = merged_traces.last() {
                self.build_children_recursive(&child_node, &last_trace.sources);
            }
        }
    }

    fn truncate_text(text: &str, max_len: usize) -> String {
        if text.chars().count() <= max_len {
            return text.to_string();
        }
        let truncated: String = text.chars().take(max_len - 3).collect();
        format!("{}...", truncated)
    }

    fn format_trace_line(trace: &TracePath) -> String {
        let type_str = match &trace.trace_type {
            TraceType::MemToReg(addr) => format!("📥 Mem->Reg ({})", addr),
            TraceType::RegToMem(reg) => format!("📤 Reg->Mem ({})", reg),
            TraceType::RegToReg(reg) => format!("🔄 Reg->Reg ({})", reg),
            TraceType::Arith(regs) => format!("🧮 Arith ({})", regs.join(",")),
            TraceType::Constant => "🎯 Constant".to_string(),
            TraceType::Unknown => "❓ Unknown".to_string(),
        };
        
        let instruction_short = trace.instruction.split(';').take(4).collect::<Vec<_>>().join(";");
        let line = format!("[{}] {} | {}", trace.line_num + 1, type_str, instruction_short);
        Self::truncate_text(&line, MAX_LINE_LENGTH)
    }

    fn create_merged_node(&mut self, traces: &[TracePath]) -> TraceNode {
        let mut display_text = String::new();
        
        if traces.len() == 1 {
            display_text = Self::format_trace_line(&traces[0]);
        } else {
            for (i, trace) in traces.iter().enumerate() {
                if i > 0 {
                    display_text.push('\n');
                }
                display_text.push_str(&Self::format_trace_line(trace));
            }
        }
        
        TraceNode {
            id: self.next_id(),
            display_text,
        }
    }

    pub fn render(&self) {
        self.tree.render();
    }

    pub fn to_graph(&self) -> Graph {
        self.tree.to_graph()
    }
}

impl Default for TracePathTree {
    fn default() -> Self {
        TracePathTree::new()
    }
}
