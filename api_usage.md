# 命令行使用指南

这个文档展示了如何使用large-text-viewer的命令行功能，这些功能复用了GUI版本的核心组件，但提供了无需前端渲染的高性能文本处理能力。

## 编译和运行

```bash
# 编译项目
cargo build --release

# 运行CLI版本（有命令行参数时自动切换）
./target/release/large-text-viewer --help
```

## 命令行功能

### 1. 文件信息 (info)

显示文件的基本信息，如大小、行数等。

```bash

# 基本文件信息
./target/release/large-text-viewer info --file /Users/teng/PycharmProjects/pythonProject/tiktok/log/record_00_XG.txt
```

输出示例：
```
File: /Users/teng/PycharmProjects/pythonProject/tiktok/log/record_00_XG.txt
Size: 14794093 bytes
Lines: 254800
```

### 2. 行提取 (lines)

从文件中提取指定范围的行，支持大文件的高效随机访问。

```bash
# 提取第1-100行
./target/release/large-text-viewer lines --file /Users/teng/PycharmProjects/pythonProject/tiktok/log/record_00_XG.txt --start 1 --end 100

# 从第1000行开始提取50行
./target/release/large-text-viewer lines --file /Users/teng/PycharmProjects/pythonProject/tiktok/log/record_00_XG.txt --start 1000 --count 50

# 显示行号 (默认使用这个)
./target/release/large-text-viewer lines --file  /Users/teng/PycharmProjects/pythonProject/tiktok/log/record_00_XG.txt  --start 1 --count 10 --line-numbers

# 提取单行
./target/release/large-text-viewer lines --file /Users/teng/PycharmProjects/pythonProject/tiktok/log/record_00_XG.txt --start 42 --count 1

# 提取某行的context

```

### 3. 文本搜索 (search)

在大文件中进行高性能搜索，支持正则表达式和并行处理。

```bash
# 基本文本搜索
./target/release/large-text-viewer search --file /Users/teng/PycharmProjects/pythonProject/tiktok/log/record_00_XG.txt --pattern "string" --max-results 50000 --count-only

# 正则表达式搜索
./target/release/large-text-viewer search --file /path/to/large.txt --pattern "\d{4}-\d{2}-\d{2}" --regex

# 区分大小写搜索
./target/release/large-text-viewer search --file /path/to/large.txt --pattern "Error" --case-sensitive

# 只统计匹配数量
./target/release/large-text-viewer search --file /path/to/large.txt --pattern "error" --count-only

# 限制结果数量
./target/release/large-text-viewer search --file /path/to/large.txt --pattern "error" --max-results 50

# 显示上下文行
./target/release/large-text-viewer search --file /Users/teng/PycharmProjects/pythonProject/tiktok/log/record_00_XG.txt --pattern "8404" --context 3

# 只搜索第10行到第50行之间的匹配
./target/release/large-text-viewer search -f /Users/teng/PycharmProjects/pythonProject/tiktok/log/record_00_XG.txt -p "string__" --start 10 --end 500
```

搜索输出示例：
```
>   1042: This is an error message
    1043: Following line
--
>   2156: Another error occurred
    2157: Stack trace follows
    2158: at function xyz()

Showed 2 matches
```