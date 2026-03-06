/// ANSI 颜色常量 —— 对应 rizin 的 Color_* 宏
pub mod color {
    pub const RESET: &str = "\x1b[0m";
    pub const RED: &str = "\x1b[31m";
    pub const GREEN: &str = "\x1b[32m";
    pub const YELLOW: &str = "\x1b[33m";
    pub const BLUE: &str = "\x1b[34m";
    pub const MAGENTA: &str = "\x1b[35m";
    pub const CYAN: &str = "\x1b[36m";
    pub const WHITE: &str = "\x1b[37m";
    pub const BRIGHT_RED: &str = "\x1b[91m";
    pub const BRIGHT_GREEN: &str = "\x1b[92m";
    pub const BRIGHT_YELLOW: &str = "\x1b[93m";
    pub const BOLD: &str = "\x1b[1m";
}

/// 画布中的一个字符单元格，对应 rizin canvas.c 中字符缓冲区的一个位置
#[derive(Clone)]
pub struct Cell {
    /// 该位置的字符
    pub ch: char,
    /// 该位置的 ANSI 颜色属性（对应 c->attr）
    pub attr: Option<&'static str>,
}

impl Default for Cell {
    fn default() -> Self {
        Cell { ch: ' ', attr: None }
    }
}

// ── 盒绘字符方向合并（解决多条边共用同一格时的字符覆盖问题）────
// 每个盒绘字符编码为方向标志的组合（上/下/左/右），
// 当两个字符重叠时 OR 合并方向标志再查表得到正确的交叉/分支字符。
const DIR_UP: u8 = 1;
const DIR_DOWN: u8 = 2;
const DIR_LEFT: u8 = 4;
const DIR_RIGHT: u8 = 8;

/// 把盒绘字符转为方向标志集合；非盒绘字符返回 0
fn char_dirs(ch: char) -> u8 {
    match ch {
        '│' => DIR_UP | DIR_DOWN,
        '─' => DIR_LEFT | DIR_RIGHT,
        '┌' => DIR_DOWN | DIR_RIGHT,
        '┐' => DIR_DOWN | DIR_LEFT,
        '└' => DIR_UP | DIR_RIGHT,
        '┘' => DIR_UP | DIR_LEFT,
        '├' => DIR_UP | DIR_DOWN | DIR_RIGHT,
        '┤' => DIR_UP | DIR_DOWN | DIR_LEFT,
        '┬' => DIR_LEFT | DIR_RIGHT | DIR_DOWN,
        '┴' => DIR_LEFT | DIR_RIGHT | DIR_UP,
        '┼' => DIR_UP | DIR_DOWN | DIR_LEFT | DIR_RIGHT,
        _ => 0,
    }
}

/// 把方向标志集合转回对应的盒绘字符
fn dirs_char(dirs: u8) -> char {
    match dirs {
        d if d == DIR_UP | DIR_DOWN => '│',
        d if d == DIR_LEFT | DIR_RIGHT => '─',
        d if d == DIR_DOWN | DIR_RIGHT => '┌',
        d if d == DIR_DOWN | DIR_LEFT => '┐',
        d if d == DIR_UP | DIR_RIGHT => '└',
        d if d == DIR_UP | DIR_LEFT => '┘',
        d if d == DIR_UP | DIR_DOWN | DIR_RIGHT => '├',
        d if d == DIR_UP | DIR_DOWN | DIR_LEFT => '┤',
        d if d == DIR_LEFT | DIR_RIGHT | DIR_DOWN => '┬',
        d if d == DIR_LEFT | DIR_RIGHT | DIR_UP => '┴',
        d if d == DIR_UP | DIR_DOWN | DIR_LEFT | DIR_RIGHT => '┼',
        _ => '┼', // fallback：多方向合并
    }
}

/// 2D 字符画布，对应 rizin 的 `RzConsCanvas`
///
/// rizin 原版用 `c->b[y]`（char 数组）存储每一行，用 `c->attrs`（哈希表）存储颜色。
/// 这里改用 `Vec<Vec<Cell>>` 统一存储字符和颜色，逻辑完全等价。
pub struct Canvas {
    /// 画布宽度（列数）
    pub w: i32,
    /// 画布高度（行数）
    pub h: i32,
    /// 字符单元格缓冲区 cells[y][x]
    cells: Vec<Vec<Cell>>,
    /// 当前光标列坐标（物理坐标，已含滚动偏移）
    pub cx: i32,
    /// 当前光标行坐标（物理坐标，已含滚动偏移）
    pub cy: i32,
    /// 水平滚动偏移，对应 c->sx（负值表示向右滚动）
    pub sx: i32,
    /// 垂直滚动偏移，对应 c->sy（负值表示向下滚动）
    pub sy: i32,
    /// 当前绘制颜色属性，对应 c->attr
    pub current_attr: Option<&'static str>,
    /// 是否启用 ANSI 颜色输出
    pub use_color: bool,
}

impl Canvas {
    /// 创建画布，对应 `rz_cons_canvas_new(w, h)`
    pub fn new(w: i32, h: i32) -> Self {
        assert!(w > 0 && h > 0, "Canvas dimensions must be positive");
        Canvas {
            w,
            h,
            cells: vec![vec![Cell::default(); w as usize]; h as usize],
            cx: 0,
            cy: 0,
            sx: 0,
            sy: 0,
            current_attr: None,
            use_color: true,
        }
    }

    /// 清空画布，对应 `rz_cons_canvas_clear(c)`
    pub fn clear(&mut self) {
        for row in &mut self.cells {
            for cell in row.iter_mut() {
                *cell = Cell::default();
            }
        }
    }

    /// 将光标移到逻辑坐标 (x, y)，内部加上滚动偏移后存入 cx/cy。
    /// 对应 `rz_cons_canvas_gotoxy(c, x, y)`。
    /// 返回 true 表示坐标在画布范围内。
    pub fn goto(&mut self, x: i32, y: i32) -> bool {
        let ax = x + self.sx;
        let ay = y + self.sy;
        if ax < 0 || ay < 0 || ax >= self.w || ay >= self.h {
            false
        } else {
            self.cx = ax;
            self.cy = ay;
            true
        }
    }

    /// 在当前光标位置写入一个字符，cx 自动右移。
    fn put(&mut self, ch: char) {
        if self.cx >= 0 && self.cy >= 0 && self.cx < self.w && self.cy < self.h {
            self.cells[self.cy as usize][self.cx as usize] = Cell {
                ch,
                attr: self.current_attr,
            };
            self.cx += 1;
        }
    }

    /// 在当前光标位置写入盒绘字符，与已有的盒绘字符智能合并。
    /// 例如：已有 `┌`（↓+→），再写 `│`（↑+↓），合并为 `├`（↑+↓+→）。
    /// 非盒绘字符（空格、文字等）直接覆盖。
    pub fn put_merge(&mut self, ch: char) {
        if self.cx >= 0 && self.cy >= 0 && self.cx < self.w && self.cy < self.h {
            let x = self.cx as usize;
            let y = self.cy as usize;
            let existing = self.cells[y][x].ch;
            let old_dirs = char_dirs(existing);
            let new_dirs = char_dirs(ch);
            let merged = if old_dirs != 0 && new_dirs != 0 {
                dirs_char(old_dirs | new_dirs)
            } else {
                ch
            };
            self.cells[y][x] = Cell {
                ch: merged,
                attr: self.current_attr,
            };
            self.cx += 1;
        }
    }

    /// 从当前光标位置写入字符串（支持换行符 `\n`，换行时 x 重置到原始列）。
    /// 对应 `rz_cons_canvas_write(c, s)`。
    pub fn write(&mut self, s: &str) {
        let orig_x = self.cx;
        for ch in s.chars() {
            if ch == '\n' {
                self.cy += 1;
                self.cx = orig_x;
                if self.cy >= self.h {
                    break;
                }
            } else {
                self.put(ch);
            }
        }
    }

    /// 用指定字符填充一个矩形区域，对应 `rz_cons_canvas_fill(c, x, y, w, h, ch)`
    pub fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, ch: char) {
        for row in 0..h {
            for col in 0..w {
                if self.goto(x + col, y + row) {
                    self.put(ch);
                }
            }
        }
    }

    /// 绘制矩形边框（Unicode 盒子线条字符），对应 `rz_cons_canvas_box(c, x, y, w, h, color)`
    ///
    /// 使用的字符：
    /// - 角点：`┌ ┐ └ ┘`
    /// - 水平边：`─`
    /// - 垂直边：`│`
    pub fn draw_box(&mut self, x: i32, y: i32, w: i32, h: i32, attr: Option<&'static str>) {
        if w < 2 || h < 2 {
            return;
        }
        let prev = self.current_attr;
        self.current_attr = attr;

        // 顶边: ┌────┐
        if self.goto(x, y) {
            self.put('┌');
            for _ in 0..w - 2 {
                self.put('─');
            }
            self.put('┐');
        }
        // 底边: └────┘
        if self.goto(x, y + h - 1) {
            self.put('└');
            for _ in 0..w - 2 {
                self.put('─');
            }
            self.put('┘');
        }
        // 左右边: │
        for i in 1..h - 1 {
            if self.goto(x, y + i) {
                self.put('│');
            }
            if self.goto(x + w - 1, y + i) {
                self.put('│');
            }
        }

        self.current_attr = prev;
    }

    /// 在节点边框内绘制标题分隔线（├────┤）
    pub fn draw_separator(&mut self, x: i32, y: i32, w: i32, attr: Option<&'static str>) {
        if w < 2 {
            return;
        }
        let prev = self.current_attr;
        self.current_attr = attr;
        if self.goto(x, y) {
            self.put('├');
            for _ in 0..w - 2 {
                self.put('─');
            }
            self.put('┤');
        }
        self.current_attr = prev;
    }

    /// 绘制从 (x1,y1) 到 (x2,y2) 的正交折线边（L 形路由）。
    /// 对应 rizin 的 `rz_cons_canvas_line_square_defined`。
    ///
    /// 路由策略：
    /// 1. 从起点 (x1, y1) 向下画竖线到 bend_y
    /// 2. 在 bend_y 行画水平线到 x2
    /// 3. 从 bend_y 继续向下到终点 (x2, y2)，末尾加箭头 ▼
    ///
    /// 所有线条字符使用 `put_merge` 智能合并，避免后画的边覆盖先画的拐角。
    pub fn draw_edge(
        &mut self,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        bend_y: i32,
        attr: Option<&'static str>,
    ) {
        let prev = self.current_attr;
        self.current_attr = attr;

        if x1 == x2 {
            // 直线：垂直向下
            for y in y1..y2 {
                if self.goto(x1, y) {
                    self.put_merge('│');
                }
            }
        } else {
            // 第一段：从起点向下到 bend_y
            for y in y1..bend_y {
                if self.goto(x1, y) {
                    self.put_merge('│');
                }
            }

            // 第二段：水平线（含转角字符）
            let (lx, rx) = (x1.min(x2), x1.max(x2));
            if self.goto(x1, bend_y) {
                // 左转角：从垂直方向来，向右 = └；向左 = ┘
                self.put_merge(if x1 < x2 { '└' } else { '┘' });
            }
            for x in lx + 1..rx {
                if self.goto(x, bend_y) {
                    self.put_merge('─');
                }
            }
            if self.goto(x2, bend_y) {
                // 右转角：向下走 + 来自左边 = ┐；来自右边 = ┌
                self.put_merge(if x1 < x2 { '┐' } else { '┌' });
            }

            // 第三段：从 bend_y+1 向下到终点
            for y in bend_y + 1..y2 {
                if self.goto(x2, y) {
                    self.put_merge('│');
                }
            }
        }

        // 终点箭头（直接覆盖，不合并）
        if self.goto(x2, y2) {
            self.put('▼');
        }

        self.current_attr = prev;
    }

    /// 绘制回边（backedge）：从节点底部出发，绕到节点左侧或右侧向上，再连到目标节点顶部。
    /// 对应 rizin 的 `rz_cons_canvas_line_back_edge`。
    pub fn draw_back_edge(
        &mut self,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        margin: i32,
        attr: Option<&'static str>,
    ) {
        let prev = self.current_attr;
        self.current_attr = attr;

        // 向下画到底部出口行
        let bottom = y1 + 1;
        for y in y1..bottom {
            if self.goto(x1, y) {
                self.put('│');
            }
        }

        // 决定绕行方向（绕到右侧）
        let side_x = x1.max(x2) + margin;

        // 从 x1 到 side_x 的水平线（底部）
        let (lx1, rx1) = (x1.min(side_x), x1.max(side_x));
        if self.goto(x1, bottom) {
            self.put(if x1 < side_x { '└' } else { '┘' });
        }
        for x in lx1 + 1..rx1 {
            if self.goto(x, bottom) {
                self.put('─');
            }
        }
        if self.goto(side_x, bottom) {
            self.put(if x1 < side_x { '┐' } else { '┌' });
        }

        // 垂直线：从 bottom 向上到 y2
        for y in y2..bottom {
            if self.goto(side_x, y) {
                self.put('│');
            }
        }

        // 从 side_x 到 x2 的水平线（顶部）
        let (lx2, rx2) = (x2.min(side_x), x2.max(side_x));
        if self.goto(side_x, y2) {
            self.put(if side_x < x2 { '└' } else { '┘' });
        }
        for x in lx2 + 1..rx2 {
            if self.goto(x, y2) {
                self.put('─');
            }
        }
        if self.goto(x2, y2) {
            self.put(if side_x < x2 { '┐' } else { '┌' });
        }

        // 箭头指向目标节点顶部
        if self.goto(x2, y2 + 1) {
            self.put('▼');
        }

        self.current_attr = prev;
    }

    /// 将画布内容序列化为带 ANSI 颜色的字符串。
    /// 对应 rizin 的 `rz_cons_canvas_to_string` + `rz_cons_canvas_print`。
    pub fn to_string_output(&self) -> String {
        let mut out = String::new();
        let mut prev_attr: Option<&'static str> = None;

        for y in 0..self.h as usize {
            // 找到本行最后一个非空字符的位置（右侧裁剪，避免大量尾随空格）
            let end = (0..self.w as usize)
                .rev()
                .find(|&x| self.cells[y][x].ch != ' ')
                .map(|x| x + 1)
                .unwrap_or(0);

            for x in 0..end {
                let cell = &self.cells[y][x];
                if self.use_color {
                    // 颜色发生变化时，先 reset 再设置新颜色
                    if prev_attr != cell.attr {
                        if prev_attr.is_some() {
                            out.push_str(color::RESET);
                        }
                        if let Some(a) = cell.attr {
                            out.push_str(a);
                        }
                        prev_attr = cell.attr;
                    }
                }
                out.push(cell.ch);
            }

            // 行末重置颜色
            if self.use_color && prev_attr.is_some() {
                out.push_str(color::RESET);
                prev_attr = None;
            }
            out.push('\n');
        }

        // 去掉末尾多余的空行
        while out.ends_with("\n\n") {
            out.pop();
        }

        out
    }
}
