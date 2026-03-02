// taint_engine.rs
use crate::search_service::{SearchService, SearchConfig};
use anyhow::Result;
use std::collections::HashSet;

pub struct TaintEngine {
    service: SearchService,
    max_depth: usize,
    visited: HashSet<usize>,
}

#[derive(Debug, Clone)]
pub struct TracePath {
    pub line_num: usize,
    pub instruction: String,
    pub trace_type: &'static str,
    pub depth: usize,
    pub sources: Vec<TracePath>,
}

impl TaintEngine {
    pub fn new(service: SearchService) -> Self {
        Self {
            service,
            max_depth: 10,
            visited: HashSet::new(),
        }
    }

    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = depth;
        self
    }

    /// 主入口：从指定行开始反向追踪
    pub fn trace_backward(&mut self, start_line: usize, target: &str) -> Result<Option<TracePath>> {
        self.visited.clear();
        Ok(self._trace_backward(start_line, target, 0))
    }

    fn _trace_backward(&mut self, line_num: usize, target: &str, depth: usize) -> Option<TracePath> {
        if depth >= self.max_depth || self.visited.contains(&line_num) {
            return None;
        }
        self.visited.insert(line_num);

        let line_text = self.service.get_line_text(line_num)?;

        let mut current = TracePath {
            line_num,
            instruction: line_text.clone(),
            trace_type: "UNKNOWN",
            depth,
            sources: vec![],
        };

        // 处理内存读取 (ld)
        if line_text.contains("ld__") {
            // 提取内存地址（去掉大小后缀）
            if let Some(ld_addr) = line_text.split(';').find(|p| p.contains("ld__")) {
                let base_addr = ld_addr.rsplit('_').nth(1).unwrap_or(ld_addr);
                println!("[ldr] 行{} 查找内存写入: {}_*", line_num + 1, base_addr);

                // 使用正则匹配同一基地址的任何写入
                let st_pattern = format!("st__{}_[0-9]+", base_addr);
                let config = SearchConfig::new(st_pattern)
                    .with_regex(true)
                    .with_max_results(1)
                    .with_line_range(None, Some(line_num));

                if let Some(prev) = self.service.find_prev(line_num, config) {
                    current.trace_type = "MEM→REG";
                    if let Some(source) = self._trace_backward(prev.line_number, base_addr, depth + 1) {
                        current.sources = vec![source];
                    }
                } else {
                    current.trace_type = "MEM→REG(END)";
                }
            }

        // 处理内存写入 (st)
        } else if line_text.contains("st__") {
            // 查找寄存器来源
            if let Some(reg) = line_text.split(';')
                .find(|p| p.starts_with("rr__"))
                .and_then(|s| s.split('=').next())
                .and_then(|s| s.strip_prefix("rr__"))
            {
                println!("[str] 行{} 查找寄存器写入: r[wr]__{}=", line_num + 1, reg);

                let config = SearchConfig::new(format!("r[wr]__{}=", reg))
                    .with_regex(true)
                    .with_max_results(1)
                    .with_line_range(None, Some(line_num));

                if let Some(prev) = self.service.find_prev(line_num, config) {
                    current.trace_type = "REG→MEM";
                    if let Some(source) = self._trace_backward(prev.line_number, reg, depth + 1) {
                        current.sources = vec![source];
                    }
                } else {
                    current.trace_type = "REG→MEM(END)";
                }
            }

        // 处理寄存器传递 (mov)
        } else if line_text.contains("mov") || line_text.contains("ldr") {
            if let Some(src_reg) = line_text.split(';')
                .find(|p| p.starts_with("rr__"))
                .and_then(|s| s.split('=').next())
                .and_then(|s| s.strip_prefix("rr__"))
            {
                println!("[mov/ldr] 行{} 查找源寄存器: {}", line_num + 1, src_reg);

                let config = SearchConfig::new(format!("r[wr]__{}=", src_reg))
                    .with_regex(true)
                    .with_max_results(1)
                    .with_line_range(None, Some(line_num));

                if let Some(prev) = self.service.find_prev(line_num, config) {
                    current.trace_type = "REG→REG";
                    if let Some(source) = self._trace_backward(prev.line_number, src_reg, depth + 1) {
                        current.sources = vec![source];
                    }
                } else {
                    current.trace_type = "REG→REG(END)";
                }
            }

        // 处理算术运算
        } else if line_text.contains("add") || line_text.contains("sub") {
            println!("[arith] 行{} 算术运算", line_num + 1);

            let src_regs: Vec<String> = line_text.split(';')
                .filter(|p| p.starts_with("rr__"))
                .filter_map(|s| s.split('=').next())
                .filter_map(|s| s.strip_prefix("rr__"))
                .map(|s| s.to_string())
                .collect();

            current.trace_type = "ARITH";
            for reg in src_regs {
                let config = SearchConfig::new(format!("r[wr]__{}=", reg))
                    .with_regex(true)
                    .with_max_results(1)
                    .with_line_range(None, Some(line_num));

                if let Some(prev) = self.service.find_prev(line_num, config) {
                    if let Some(source) = self._trace_backward(prev.line_number, &reg, depth + 1) {
                        current.sources.push(source);
                    }
                }
            }

        } else {
            current.trace_type = "CONST/END";
        }

        Some(current)
    }
}

impl TracePath {
    pub fn print(&self) {
        self._print(0);
    }

    fn _print(&self, indent: usize) {
        let indent_str = "  ".repeat(indent);
        println!("{}{} [行{}] {}",
                 indent_str,
                 self.trace_type,
                 self.line_num + 1,
                 self.instruction
        );
        for src in &self.sources {
            src._print(indent + 1);
        }
    }
}

pub fn test_taint() -> anyhow::Result<()> {
    use large_text_core::file_reader::FileReader;

    let file_path = std::path::PathBuf::from("/Users/teng/RustroverProjects/large-text-viewer/logs/record.csv");
    let reader = FileReader::new(file_path, encoding_rs::UTF_8)?;
    let service = SearchService::new(reader);

    let mut engine = TaintEngine::new(service).with_max_depth(5);

    println!("\n=== 追踪内存地址: ld__6cf01586a0_4 ===\n");
    if let Some(trace) = engine.trace_backward(9028, "ld__6cf01586a0_4")? {
        trace.print();
    }

    Ok(())
}