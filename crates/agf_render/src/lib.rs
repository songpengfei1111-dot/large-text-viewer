/// agf_render —— Rizin agf 图形渲染器的 Rust 重写版
///
/// 这是一个用于渲染控制流图（CFG）的库，支持 Sugiyama 分层布局算法。

pub mod canvas;
pub mod graph;
pub mod layout;
pub mod render;

// 重新导出常用类型，方便使用
pub use graph::{Edge, EdgeColor, Graph, Node, NodeId};
pub use layout::layout;
pub use render::{render, render_to_stdout};
