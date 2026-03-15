use agf_render::{Graph, EdgeColor, layout, render_to_stdout};
use crate::taint_engine::TracePath;
use crate::insn_analyzer::ParsedInsn;
use std::fmt;

const MAX_LINE_LENGTH: usize = 45;

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
        let title = "";
        let body = value_str;
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
        
        let is_end = if let Some(last_trace) = merged_traces.last() {
            last_trace.sources.is_empty()
        } else {
            false
        };
        
        let end_reason = if is_end {
            Some(Self::get_end_reason(merged_traces.last().unwrap()))
        } else {
            None
        };
        
        let root_node = self.create_merged_node(&merged_traces, end_reason.as_deref());
        self.tree.add_root(root_node.clone());
        
        if let Some(last_trace) = merged_traces.last() {
            if !last_trace.sources.is_empty() {
                self.build_children_recursive(&root_node, &last_trace.sources);
            }
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
            
            let is_end = if let Some(last_trace) = merged_traces.last() {
                last_trace.sources.is_empty()
            } else {
                false
            };
            
            let end_reason = if is_end {
                Some(Self::get_end_reason(merged_traces.last().unwrap()))
            } else {
                None
            };
            
            let child_node = self.create_merged_node(&merged_traces, end_reason.as_deref());
            let is_left = idx % 2 == 0;
            self.tree.add_child(parent.clone(), child_node.clone(), is_left);
            
            if let Some(last_trace) = merged_traces.last() {
                if !last_trace.sources.is_empty() {
                    self.build_children_recursive(&child_node, &last_trace.sources);
                }
            }
        }
    }
    
    fn get_end_reason(trace: &TracePath) -> String {
        match &trace.trace_type {
            crate::taint_engine::TraceType::Constant => "Constant value or end point".to_string(),
            crate::taint_engine::TraceType::Unknown => "Unknown instruction type".to_string(),
            _ => "End of trace".to_string(),
        }
    }

    fn truncate_text(text: &str, max_len: usize) -> String {
        if text.chars().count() <= max_len {
            return text.to_string();
        }
        let truncated: String = text.chars().take(max_len - 3).collect();
        format!("{}...", truncated)
    }

    // 用来设定图中显示文本的格式
    fn format_trace_line(trace: &TracePath) -> String {
        let parts: Vec<&str> = trace.instruction.split(';').collect();
        
        let line_num = trace.line_num + 1;
        let insn_name = parts.get(3).unwrap_or(&"").trim();
        let insn_opt = parts.get(4).unwrap_or(&"").trim();

        let mut mem_info = String::new();
        if let Some(parsed) = &trace.parsed_insn {
            if let Some(addr) = parsed.mem_addr {
                mem_info = format!(" @0x{:x}", addr);
            }
        }
        
        let line = format!("{}:{} {}", line_num, insn_name, insn_opt);
        Self::truncate_text(&line, MAX_LINE_LENGTH)
    }

    fn create_merged_node(&mut self, traces: &[TracePath], end_reason: Option<&str>) -> TraceNode {
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
        
        if let Some(reason) = end_reason {
            if !display_text.is_empty() {
                display_text.push('\n');
            }
            display_text.push_str(&format!("[end] : {}", reason));
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
