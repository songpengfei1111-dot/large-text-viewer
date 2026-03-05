// shadow_memory.rs
// Shadow Memory 模块：字节级污点追踪
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TaintTag(u32);

impl TaintTag {
    pub fn id(&self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone)]
pub struct ShadowMemory {
    // 字节级别的污点标记：地址 -> 污点标签
    mem_tags: HashMap<u64, TaintTag>,
    // 寄存器污点标记：寄存器名 -> 字节级污点数组
    reg_tags: HashMap<String, Vec<Option<TaintTag>>>,
    next_tag: u32,
}

impl ShadowMemory {
    pub fn new() -> Self {
        Self {
            mem_tags: HashMap::new(),
            reg_tags: HashMap::new(),
            next_tag: 1,
        }
    }

    /// 生成新的污点标签
    pub fn new_tag(&mut self) -> TaintTag {
        let tag = TaintTag(self.next_tag);
        self.next_tag += 1;
        tag
    }

    /// 标记内存字节范围
    pub fn taint_memory(&mut self, addr: u64, size: usize, tag: TaintTag) {
        for i in 0..size {
            self.mem_tags.insert(addr + i as u64, tag);
        }
    }

    /// 检查内存是否被污染，返回每个字节的污点标签
    pub fn is_memory_tainted(&self, addr: u64, size: usize) -> Vec<Option<TaintTag>> {
        (0..size)
            .map(|i| self.mem_tags.get(&(addr + i as u64)).copied())
            .collect()
    }

    /// 清除内存污点
    pub fn clear_memory(&mut self, addr: u64, size: usize) {
        for i in 0..size {
            self.mem_tags.remove(&(addr + i as u64));
        }
    }

    /// 标记寄存器（支持部分字节）
    /// byte_offset: 从寄存器的第几个字节开始
    /// size: 标记多少个字节
    pub fn taint_register(&mut self, reg: &str, byte_offset: usize, size: usize, tag: TaintTag) {
        let tags = self.get_or_create_reg_tags(reg);
        
        for i in 0..size {
            if byte_offset + i < tags.len() {
                tags[byte_offset + i] = Some(tag);
            }
        }
    }

    /// 获取或创建寄存器的污点标签数组
    fn get_or_create_reg_tags(&mut self, reg: &str) -> &mut Vec<Option<TaintTag>> {
        let reg_size = Self::get_reg_size(reg);
        self.reg_tags.entry(reg.to_string())
            .or_insert_with(|| vec![None; reg_size])
    }

    /// 获取寄存器污点（返回每个字节的污点标签）
    pub fn get_register_taint(&self, reg: &str) -> Vec<Option<TaintTag>> {
        self.reg_tags.get(reg)
            .cloned()
            .unwrap_or_else(|| vec![None; Self::get_reg_size(reg)])
    }

    /// 清除寄存器污点
    pub fn clear_register(&mut self, reg: &str) {
        *self.get_or_create_reg_tags(reg) = vec![None; Self::get_reg_size(reg)];
    }

    /// 检查寄存器是否被污染
    pub fn is_register_tainted(&self, reg: &str) -> bool {
        self.reg_tags.get(reg)
            .map(|tags| tags.iter().any(|t| t.is_some()))
            .unwrap_or(false)
    }

    /// ARM64 寄存器大小映射
    fn get_reg_size(reg: &str) -> usize {
        if reg.is_empty() {
            return 8;
        }
        
        match reg.chars().next() {
            Some('x') => 8,   // 64-bit 通用寄存器
            Some('w') => 4,   // 32-bit 通用寄存器
            Some('q') => 16,  // 128-bit SIMD 寄存器
            Some('d') => 8,   // 64-bit SIMD 寄存器
            Some('s') => 4,   // 32-bit SIMD 寄存器
            Some('h') => 2,   // 16-bit 寄存器
            Some('b') => 1,   // 8-bit 寄存器
            _ => 8,           // 默认 64-bit
        }
    }

    /// 处理寄存器别名关系（x0 和 w0 共享低32位）
    pub fn sync_register_aliases(&mut self, reg: &str) {
        if reg.is_empty() {
            return;
        }

        match reg.chars().next() {
            Some('x') => self.sync_x_to_w(reg),
            Some('w') => self.sync_w_to_x(reg),
            Some('q') => self.sync_q_to_lower(reg),
            Some('d') => self.sync_d_to_others(reg),
            Some('s') => self.sync_s_to_others(reg),
            _ => {}
        }
    }

    fn sync_x_to_w(&mut self, x_reg: &str) {
        if let Some(num) = x_reg.strip_prefix('x') {
            let w_reg = format!("w{}", num);
            if let Some(x_tags) = self.reg_tags.get(x_reg).cloned() {
                let w_tags: Vec<_> = x_tags.iter().take(4).copied().collect();
                self.reg_tags.insert(w_reg, w_tags);
            }
        }
    }

    fn sync_w_to_x(&mut self, w_reg: &str) {
        if let Some(num) = w_reg.strip_prefix('w') {
            let x_reg = format!("x{}", num);
            if let Some(w_tags) = self.reg_tags.get(w_reg).cloned() {
                let x_tags = self.get_or_create_reg_tags(&x_reg);
                for i in 0..4.min(w_tags.len()) {
                    x_tags[i] = w_tags[i];
                }
            }
        }
    }

    fn sync_q_to_lower(&mut self, q_reg: &str) {
        if let Some(num) = q_reg.strip_prefix('q') {
            if let Some(q_tags) = self.reg_tags.get(q_reg).cloned() {
                let d_reg = format!("d{}", num);
                let s_reg = format!("s{}", num);
                self.reg_tags.insert(d_reg, q_tags.iter().take(8).copied().collect());
                self.reg_tags.insert(s_reg, q_tags.iter().take(4).copied().collect());
            }
        }
    }

    fn sync_d_to_others(&mut self, d_reg: &str) {
        if let Some(num) = d_reg.strip_prefix('d') {
            if let Some(d_tags) = self.reg_tags.get(d_reg).cloned() {
                let q_reg = format!("q{}", num);
                let q_tags = self.get_or_create_reg_tags(&q_reg);
                for i in 0..8.min(d_tags.len()) {
                    q_tags[i] = d_tags[i];
                }
                
                let s_reg = format!("s{}", num);
                self.reg_tags.insert(s_reg, d_tags.iter().take(4).copied().collect());
            }
        }
    }

    fn sync_s_to_others(&mut self, s_reg: &str) {
        if let Some(num) = s_reg.strip_prefix('s') {
            if let Some(s_tags) = self.reg_tags.get(s_reg).cloned() {
                let d_reg = format!("d{}", num);
                let d_tags = self.get_or_create_reg_tags(&d_reg);
                for i in 0..4.min(s_tags.len()) {
                    d_tags[i] = s_tags[i];
                }
                
                let q_reg = format!("q{}", num);
                let q_tags = self.get_or_create_reg_tags(&q_reg);
                for i in 0..4.min(s_tags.len()) {
                    q_tags[i] = s_tags[i];
                }
            }
        }
    }

    /// 寄存器到寄存器的污点传播
    pub fn propagate_reg_to_reg(&mut self, from: &str, to: &str) {
        if let Some(from_tags) = self.reg_tags.get(from).cloned() {
            let to_size = Self::get_reg_size(to);
            let from_size = from_tags.len();
            
            // 根据目标寄存器大小截取或扩展
            let propagated: Vec<_> = if to_size <= from_size {
                from_tags.iter().take(to_size).copied().collect()
            } else {
                let mut tags = from_tags.clone();
                tags.resize(to_size, None);
                tags
            };
            
            self.reg_tags.insert(to.to_string(), propagated);
            self.sync_register_aliases(to);
        }
    }

    /// 内存到寄存器的污点传播
    pub fn propagate_mem_to_reg(&mut self, mem_addr: u64, size: usize, reg: &str, reg_offset: usize) {
        let mem_tags = self.is_memory_tainted(mem_addr, size);
        
        for (i, tag) in mem_tags.iter().enumerate() {
            if let Some(t) = tag {
                self.taint_register(reg, reg_offset + i, 1, *t);
            }
        }
        
        self.sync_register_aliases(reg);
    }

    /// 寄存器到内存的污点传播
    pub fn propagate_reg_to_mem(&mut self, reg: &str, reg_offset: usize, mem_addr: u64, size: usize) {
        let reg_tags = self.get_register_taint(reg);
        
        for i in 0..size {
            if let Some(Some(tag)) = reg_tags.get(reg_offset + i) {
                self.taint_memory(mem_addr + i as u64, 1, *tag);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_alias() {
        let mut shadow = ShadowMemory::new();
        let tag = shadow.new_tag();
        
        // 标记 x0 的低4字节
        shadow.taint_register("x0", 0, 4, tag);
        shadow.sync_register_aliases("x0");
        
        // w0 应该被污染
        assert!(shadow.is_register_tainted("w0"));
        
        // 验证 w0 的所有字节都被污染
        let w0_tags = shadow.get_register_taint("w0");
        assert_eq!(w0_tags.len(), 4);
        assert!(w0_tags.iter().all(|t| t.is_some()));
    }

    #[test]
    fn test_simd_register_alias() {
        let mut shadow = ShadowMemory::new();
        let tag = shadow.new_tag();
        
        // 标记 q0 的前4字节
        shadow.taint_register("q0", 0, 4, tag);
        shadow.sync_register_aliases("q0");
        
        // d0 和 s0 应该被污染
        assert!(shadow.is_register_tainted("d0"));
        assert!(shadow.is_register_tainted("s0"));
    }

    #[test]
    fn test_mem_to_reg_propagation() {
        let mut shadow = ShadowMemory::new();
        let tag = shadow.new_tag();
        
        // 标记内存
        let addr = 0x1000;
        shadow.taint_memory(addr, 4, tag);
        
        // 传播到寄存器
        shadow.propagate_mem_to_reg(addr, 4, "w8", 0);
        
        // 验证 w8 被污染
        assert!(shadow.is_register_tainted("w8"));
    }
}
