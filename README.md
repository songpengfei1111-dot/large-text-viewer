fork 自 acejarvis/large-text-viewer

使用 ltv-mcp 助力ai逆向

目前进度

- [x]  重构后端cli，分离搜索逻辑和参数处理
- [x]  修改trace格式，适配污点分析
- [x]  反向污点 （感谢trace-ui作者提供的思路！https://github.com/imj01y/trace-ui 请给大佬点个🌟）
- [ ]  函数层级图 （debugging）
- [ ]  正向污点
- [ ]  用polors分析指令密度，从整体上分析计算指令集中的范围，启发式判断加密算法位置