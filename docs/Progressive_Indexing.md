# 渐进式索引实现文档

## 概述

本文档描述了大文件文本查看器中实现的渐进式索引（Progressive Indexing）功能。

## 问题背景

原有实现存在以下问题：

1. **小文件（<100MB）**：全量索引，一次性加载所有行号
2. **大文件（≥100MB）**：只采样稀疏检查点（每10MB一个），通过估算平均行长来定位行
3. **大文件的缺点**：
   - 行定位不精确，需要每次动态扫描
   - `get_line_with_reader` 每次都要读取并扫描一个范围的数据
   - 对于频繁访问的行，性能较差

## 解决方案：渐进式索引

### 核心思想

对于大文件（≥100MB），采用"先快速索引一部分，然后在后台继续索引"的策略：

1. **初始阶段**：快速索引前 50MB，立即返回这部分的精确行号
2. **后台阶段**：在后台线程继续索引剩余部分，逐步填充 `line_offsets`
3. **查询时**：
   - 如果查询的行在已索引范围内 → 使用精确的 `line_offsets`
   - 如果查询的行在未索引范围内 → 使用估算模式

### 架构变更

#### 1. LineIndexer 结构体变更

```rust
pub struct LineIndexer {
    line_offsets: Arc<RwLock<Vec<usize>>>,      // 改为线程安全的共享结构
    total_lines: usize,
    indexed_up_to: Arc<AtomicUsize>,            // 新增：已索引到的字节位置
    fully_indexed: Arc<AtomicBool>,             // 新增：是否完全索引完成
    sample_interval: usize,
    file_size: usize,
    avg_line_length: f64,
    cancel_token: Arc<AtomicBool>,              // 新增：后台任务取消标记
}
```

#### 2. FileReader 变更

```rust
pub struct FileReader {
    mmap: Arc<Mmap>,  // 改为 Arc 包装，支持跨线程共享
    path: PathBuf,
    encoding: &'static Encoding,
}

impl Clone for FileReader {
    // 实现 Clone trait，支持在线程间传递
}
```

### 关键方法

#### index_file

主索引方法，根据文件大小选择策略：

```rust
pub fn index_file(&mut self, reader: &FileReader) {
    const FULL_INDEX_THRESHOLD: usize = 100_000_000; // 100 MB
    const INITIAL_INDEX_SIZE: usize = 50_000_000;    // 先索引 50MB

    if self.file_size <= FULL_INDEX_THRESHOLD {
        // 小文件：全量索引
        let data = reader.all_data();
        self.full_index(data);
        self.fully_indexed.store(true, Ordering::Relaxed);
    } else {
        // 大文件：渐进式索引
        // 1. 快速索引前 50MB
        let initial_data = reader.get_bytes(0, INITIAL_INDEX_SIZE);
        self.full_index(initial_data);
        self.indexed_up_to.store(INITIAL_INDEX_SIZE, Ordering::Relaxed);
        
        // 2. 启动后台任务继续索引
        self.start_background_indexing(reader_arc);
    }
}
```

#### start_background_indexing

后台索引任务：

```rust
fn start_background_indexing(&self, reader: Arc<FileReader>) {
    std::thread::spawn(move || {
        const CHUNK_SIZE: usize = 20_000_000; // 每次索引 20MB
        
        while current_pos < file_size {
            // 检查取消标记
            if cancel_token.load(Ordering::Relaxed) {
                break;
            }
            
            // 读取并索引一个块
            let chunk = reader.get_bytes(current_pos, chunk_end);
            let mut new_offsets = Vec::new();
            for (i, &byte) in chunk.iter().enumerate() {
                if byte == b'\n' {
                    new_offsets.push(current_pos + i + 1);
                }
            }
            
            // 添加到共享的 line_offsets
            {
                let mut offsets = line_offsets.write().unwrap();
                offsets.extend(new_offsets);
            }
            
            current_pos = chunk_end;
            indexed_up_to.store(current_pos, Ordering::Relaxed);
            
            // 短暂休眠，避免占用过多 CPU
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        
        fully_indexed.store(true, Ordering::Relaxed);
    });
}
```

#### get_line_with_reader

智能行查询方法：

```rust
pub fn get_line_with_reader(&self, line_num: usize, reader: &FileReader) -> Option<(usize, usize)> {
    let offsets = self.line_offsets.read().unwrap();
    
    // 如果行号在已索引范围内，直接使用精确索引
    if self.sample_interval == 0 && line_num < offsets.len() {
        let start = offsets[line_num];
        let end = if line_num + 1 < offsets.len() {
            offsets[line_num + 1]
        } else {
            usize::MAX
        };
        return Some((start, end));
    }
    
    // 否则使用估算和扫描
    self.get_line_with_sparse_index(line_num, reader)
}
```

### UI 集成

#### 进度显示

在状态栏显示索引进度：

```rust
fn render_status_bar(&mut self, ctx: &egui::Context) {
    // ... 其他状态信息 ...
    
    // 显示索引进度
    if !self.line_indexer.is_fully_indexed() {
        ui.separator();
        let progress = self.line_indexer.indexing_progress();
        ui.add(egui::ProgressBar::new(progress)
            .text(format!("Indexing {:.0}%", progress * 100.0)));
        ctx.request_repaint(); // 持续刷新以更新进度
    }
}
```

#### 文件切换时取消后台任务

```rust
fn open_file(&mut self, path: PathBuf) {
    // 取消之前的后台索引任务
    self.line_indexer.cancel_background_indexing();
    
    // ... 打开新文件 ...
}
```

## 性能优势

### 1. 快速响应

- 用户打开大文件后，前 50MB 的内容可以立即精确定位
- 对于只查看文件开头的场景（最常见），体验接近小文件

### 2. 渐进改善

- 随着后台索引进行，越来越多的行可以精确定位
- 用户在浏览文件时，索引会自动完成

### 3. 内存可控

- 使用 `memmap2` 的内存映射，不会一次性加载整个文件
- 后台索引分块进行（每次 20MB），避免内存峰值

### 4. CPU 友好

- 后台线程每处理一个块后休眠 10ms，避免占用过多 CPU
- 可以随时取消后台任务

## 线程安全

### 使用的同步原语

1. **Arc<RwLock<Vec<usize>>>**：用于 `line_offsets`
   - 允许多个读者同时访问
   - 写入时独占访问

2. **Arc<AtomicUsize>**：用于 `indexed_up_to`
   - 无锁原子操作
   - 高性能的进度跟踪

3. **Arc<AtomicBool>**：用于 `fully_indexed` 和 `cancel_token`
   - 无锁原子操作
   - 用于状态标记和取消信号

### 数据竞争防护

- 所有共享数据都通过 Arc 包装
- 可变访问通过 RwLock 保护
- 原子类型保证操作的原子性

## 测试

所有现有测试都已通过，包括：

- `test_line_indexer_small_file`：小文件索引测试
- `test_line_indexer_empty_lines`：空行处理测试
- 其他 file_reader、search_engine、replacer 的测试

## 未来改进方向

1. **可配置的阈值**：允许用户配置初始索引大小和块大小
2. **索引持久化**：将索引保存到磁盘，下次打开时快速加载
3. **优先级索引**：优先索引用户正在查看的区域
4. **内存限制**：对于超大文件（如 10GB+），限制最大索引行数

## 总结

渐进式索引方案成功解决了大文件打开慢的问题，同时保持了小文件的性能。通过合理的线程同步和内存管理，实现了高效、安全的后台索引功能。
