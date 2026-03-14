mod cli_core;
mod taint_engine;
mod search_service;
mod insn_analyzer;    // 新增

// use std::env;
// fn main() -> eframe::Result<()> {
    // 检查是否有命令行参数
    // let _args: Vec<String> = env::args().collect();
    // match cli_core::run_cli() {
    //     Ok(()) => return Ok(()),
    //     Err(e) => {
    //         eprintln!("CLI Error: {}", e);
    //         std::process::exit(1);
    //     }
    // }
// }

fn main() {
    // 运行演示
    // taint_demo::demo_shadow_memory();
    // taint_demo::demo_insn_analyzer();
    // taint_demo::demo_full_taint_flow();
    
    // 运行实际的污点追踪
    // let _ = taint_engine::test_taint();
    // let _ = taint_engine::test_taint_1();
    // let _ = taint_engine::test_taint_overlap();
    
    // 其他测试
    // test_reg::test_reg();
    // insn_il::test_parse_single();
    // insn_il::test_parse_instruction();

    // 测试 agf_render 功能
    // test_agf_render();
    test_binatree_render()
}



fn test_binatree(){

    #[derive(Debug, Clone)]
    struct TreeNode<T> {
        value: T,
        left: Option<Box<TreeNode<T>>>,
        right: Option<Box<TreeNode<T>>>,
    }

    #[derive(Debug)]
    struct BinaryTree<T> {
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
        fn new() -> Self {
            BinaryTree { root: None }
        }

        // 添加根节点
        fn add_root(&mut self, value: T) {
            self.root = Some(Box::new(TreeNode::new(value)));
        }

        // 通用添加节点方法 - 在指定父节点下添加子节点
        fn add_child(&mut self, parent_value: T, child_value: T, is_left: bool) -> bool {
            if let Some(root) = &mut self.root {
                Self::add_child_recursive(root, &parent_value, child_value, is_left)
            } else {
                false
            }
        }

        fn add_child_recursive(
            node: &mut Box<TreeNode<T>>,
            parent_value: &T,
            child_value: T,
            is_left: bool,
        ) -> bool {
            // 如果当前节点是父节点
            if node.value == *parent_value {
                let child = Box::new(TreeNode::new(child_value));
                if is_left {
                    if node.left.is_none() {
                        node.left = Some(child);
                        return true;
                    }
                } else {
                    if node.right.is_none() {
                        node.right = Some(child);
                        return true;
                    }
                }
                return false; // 位置已被占用
            }

            // 在左子树中查找
            if let Some(left) = &mut node.left {
                if Self::add_child_recursive(left, parent_value, child_value.clone(), is_left) {
                    return true;
                }
            }

            // 在右子树中查找
            if let Some(right) = &mut node.right {
                if Self::add_child_recursive(right, parent_value, child_value, is_left) {
                    return true;
                }
            }

            false
        }

        // 便捷方法：添加左子节点
        fn add_left(&mut self, parent_value: T, child_value: T) -> bool {
            self.add_child(parent_value, child_value, true)
        }

        // 便捷方法：添加右子节点
        fn add_right(&mut self, parent_value: T, child_value: T) -> bool {
            self.add_child(parent_value, child_value, false)
        }

        // 前序遍历
        fn preorder(&self) -> Vec<T> {
            let mut result = Vec::new();
            if let Some(root) = &self.root {
                Self::preorder_recursive(root, &mut result);
            }
            result
        }

        fn preorder_recursive(node: &Box<TreeNode<T>>, result: &mut Vec<T>) {
            result.push(node.value.clone());

            if let Some(left) = &node.left {
                Self::preorder_recursive(left, result);
            }

            if let Some(right) = &node.right {
                Self::preorder_recursive(right, result);
            }
        }

        // 后序遍历
        fn postorder(&self) -> Vec<T> {
            let mut result = Vec::new();
            if let Some(root) = &self.root {
                Self::postorder_recursive(root, &mut result);
            }
            result
        }

        fn postorder_recursive(node: &Box<TreeNode<T>>, result: &mut Vec<T>) {
            if let Some(left) = &node.left {
                Self::postorder_recursive(left, result);
            }

            if let Some(right) = &node.right {
                Self::postorder_recursive(right, result);
            }

            result.push(node.value.clone());
        }

        // 中序遍历
        fn inorder(&self) -> Vec<T> {
            let mut result = Vec::new();
            if let Some(root) = &self.root {
                Self::inorder_recursive(root, &mut result);
            }
            result
        }

        fn inorder_recursive(node: &Box<TreeNode<T>>, result: &mut Vec<T>) {
            if let Some(left) = &node.left {
                Self::inorder_recursive(left, result);
            }

            result.push(node.value.clone());

            if let Some(right) = &node.right {
                Self::inorder_recursive(right, result);
            }
        }

        // 打印树结构
        fn print(&self)
        where
            T: std::fmt::Debug
        {
            if let Some(root) = &self.root {
                println!("{:?}", root.value);
                Self::print_children(root, String::new());
            }
        }

        fn print_children(node: &Box<TreeNode<T>>, prefix: String)
        where
            T: std::fmt::Debug
        {
            let has_left = node.left.is_some();
            let has_right = node.right.is_some();

            // 处理左子节点
            if let Some(left) = &node.left {
                if has_right {
                    // 如果还有右子节点，使用 ├──
                    println!("{}├── {:?}", prefix, left.value);
                    Self::print_children(left, format!("{}│   ", prefix));
                } else {
                    // 如果没有右子节点，使用 └──
                    println!("{}└── {:?}", prefix, left.value);
                    Self::print_children(left, format!("{}    ", prefix));
                }
            }

            // 处理右子节点
            if let Some(right) = &node.right {
                // 右子节点总是最后一个，使用 └──
                println!("{}└── {:?}", prefix, right.value);
                Self::print_children(right, format!("{}    ", prefix));
            }
        }
    }



    let mut tree = BinaryTree::new();

    // 添加根节点
    tree.add_root(1);

    // 添加子节点
    tree.add_left(1, 2);   // 在节点1下添加左子节点2
    tree.add_right(1, 3);  // 在节点1下添加右子节点3

    tree.add_left(2, 4);   // 在节点2下添加左子节点4
    tree.add_right(2, 5);  // 在节点2下添加右子节点5

    tree.add_right(3, 6);  // 在节点3下添加右子节点6

    println!("树结构:");
    tree.print();

    println!("\n前序遍历: {:?}", tree.preorder());
    println!("中序遍历: {:?}", tree.inorder());
    println!("后序遍历: {:?}", tree.postorder());
}

/// 测试使用 agf_render 渲染二叉树
fn test_binatree_render() {
    use agf_render::{Graph, layout, render_to_stdout};
    use std::collections::HashMap;

    #[derive(Debug, Clone)]
    struct TreeNode<T> {
        value: T,
        left: Option<Box<TreeNode<T>>>,
        right: Option<Box<TreeNode<T>>>,
    }

    #[derive(Debug)]
    struct BinaryTree<T> {
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
        fn new() -> Self {
            BinaryTree { root: None }
        }

        fn add_root(&mut self, value: T) {
            self.root = Some(Box::new(TreeNode::new(value)));
        }

        fn add_child(&mut self, parent_value: T, child_value: T, is_left: bool) -> bool {
            if let Some(root) = &mut self.root {
                Self::add_child_recursive(root, &parent_value, child_value, is_left)
            } else {
                false
            }
        }

        fn add_child_recursive(
            node: &mut Box<TreeNode<T>>,
            parent_value: &T,
            child_value: T,
            is_left: bool,
        ) -> bool {
            if node.value == *parent_value {
                let child = Box::new(TreeNode::new(child_value));
                if is_left {
                    if node.left.is_none() {
                        node.left = Some(child);
                        return true;
                    }
                } else {
                    if node.right.is_none() {
                        node.right = Some(child);
                        return true;
                    }
                }
                return false;
            }

            if let Some(left) = &mut node.left {
                if Self::add_child_recursive(left, parent_value, child_value.clone(), is_left) {
                    return true;
                }
            }

            if let Some(right) = &mut node.right {
                if Self::add_child_recursive(right, parent_value, child_value, is_left) {
                    return true;
                }
            }

            false
        }

        fn add_left(&mut self, parent_value: T, child_value: T) -> bool {
            self.add_child(parent_value, child_value, true)
        }

        fn add_right(&mut self, parent_value: T, child_value: T) -> bool {
            self.add_child(parent_value, child_value, false)
        }
    }

    // 创建二叉树
    let mut tree = BinaryTree::new();
    tree.add_root(1);
    tree.add_left(1, 2);
    tree.add_right(1, 3);
    tree.add_left(2, 4);
    tree.add_right(2, 5);
    tree.add_right(3, 6);
    tree.add_right(6, 7);

    println!("=== 使用 agf_render 渲染二叉树 ===\n");

    // 将二叉树转换为 Graph
    let mut graph = Graph::new();

    // 递归遍历二叉树，添加节点和边
    // 使用 EdgeColor 来区分左右子节点
    fn add_tree_to_graph<T: std::fmt::Debug>(
        tree_node: &Box<TreeNode<T>>,
        graph: &mut Graph,
    ) -> usize {
        use agf_render::EdgeColor;
        
        let value_str = format!("{:?}", tree_node.value);
        let node_id = graph.add_node(&value_str, &value_str);

        // 添加左子节点（用绿色表示）
        if let Some(left) = &tree_node.left {
            let left_id = add_tree_to_graph(left, graph);
            graph.add_edge(node_id, left_id, EdgeColor::True);
        }

        // 添加右子节点（用红色表示）
        if let Some(right) = &tree_node.right {
            let right_id = add_tree_to_graph(right, graph);
            graph.add_edge(node_id, right_id, EdgeColor::False);
        }

        node_id
    }

    if let Some(root) = &tree.root {
        add_tree_to_graph(root, &mut graph);
    }

    // 执行布局算法
    layout(&mut graph);

    // 渲染到标准输出
    render_to_stdout(&graph);

    println!("\n=== 二叉树渲染完成 ===");
    println!("注意: 绿色边表示左子节点，红色边表示右子节点");
}


/// 测试 agf_render 的 CFG 渲染功能
/// 注意不能出现中文文本，不然长度计算回出现误差
fn test_agf_render() {
    use agf_render::{Graph, EdgeColor, layout, render_to_stdout};

    println!("=== 测试 agf_render CFG 渲染 ===\n");

    // 创建一个简单的控制流图
    let mut g = Graph::new();

    // 添加节点
    let entry = g.add_node(
        "entry",
        "push rbp\n\
        mov rbp, rsp\n\
        cmp eax, 0\n\
        je false_branch",
    );
    let true_branch = g.add_node(
        "true_branch",
        "mov eax, 1\n\
        jmp exit",
    );
    let false_branch = g.add_node(
        "false_branch",
        "mov eax, 0 asdlfjasdfasdkjhjkhkhk",
    );
    let exit = g.add_node(
        "exit",
        "pop rbp\nret",
    );

    // 添加边
    g.add_edge(entry, true_branch, EdgeColor::False);  // 不跳转
    g.add_edge(entry, false_branch, EdgeColor::True);  // 跳转
    g.add_edge_uncond(true_branch, exit);
    g.add_edge_uncond(false_branch, exit);


    // 执行布局算法
    layout(&mut g);

    // 渲染到标准输出
    render_to_stdout(&g);

    println!("\n=== agf_render 测试完成 ===");
}
