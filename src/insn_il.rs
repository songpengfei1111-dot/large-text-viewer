// src/insn_il.rs
use regex::Regex;

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedInstruction {
    pub raw_text: String,
    /// 绝对地址 (如: 0x6d2d750e30)
    pub abs_addr: String,
    /// so_ea (如: 0x10de30)
    pub so_ea: String,
    /// 汇编指令的机器码 (如: 6d2d750e30;10de30;6b09011f 中的 6b09011f)
    pub asm: String,
    /// 操作码 (如: cmp, mov, movk, b.eq, b.le, ldp)
    pub op: String,
    /// 操作数信息 (如: w8, w9; 或 #0x6d2d750fcc(10dfcc);)
    pub opinfo: String,
    /// 读取的寄存器数量
    pub num_reg_read: usize,
    /// 读取的寄存器列表
    pub reg_reads: Vec<String>,
    /// 内存操作信息
    pub memop: String,
    /// 写入的寄存器数量
    pub num_reg_writes: usize,
    /// 写入的寄存器列表
    pub reg_writes: Vec<String>,
}

impl ParsedInstruction {
    pub fn new() -> Self {
        Self {
            raw_text: String::new(),
            abs_addr: String::new(),
            so_ea: String::new(),
            asm: String::new(),
            op: String::new(),
            opinfo: String::new(),
            num_reg_read: 0,
            reg_reads: Vec::new(),
            memop: String::new(),
            num_reg_writes: 0,
            reg_writes: Vec::new(),
        }
    }

    /// 从一行指令日志解析
    pub fn parse(line: &str) -> Option<Self> {
        let mut inst = ParsedInstruction::new();

        inst.raw_text = line.to_string();

        // 解析主要部分: abs_addr;so_ea;asm;op;opinfo;extra
        let parts: Vec<&str> = line.split(';').collect();
        if parts.len() < 5 {
            return None;
        }

        inst.abs_addr = parts[0].to_string();
        inst.so_ea = parts[1].to_string();
        inst.asm = parts[2].to_string();
        inst.op = parts[3].to_string();
        inst.opinfo = parts[4].to_string();

        let extra = parts[5..].join(";");
        inst.parse_extra(&extra);

        Some(inst)
    }

    fn parse_extra(&mut self, extra: &str) {
        let mut reg_reads = Vec::new();
        let mut reg_writes = Vec::new();

        reg_reads = extract_reg_reads(self.raw_text.as_str());

        reg_writes = extract_reg_writes(self.raw_text.as_str());

        //memop

        self.reg_reads = reg_reads;
        self.num_reg_read = self.reg_reads.len();

        self.reg_writes = reg_writes;
        self.num_reg_writes = self.reg_writes.len();

    }

    /// 解析单条指令（对外提供的简洁接口）
    pub fn parse_single(line: &str) -> Option<Self> {
        Self::parse(line)
    }
}

fn extract_reg_reads(line_text: &str) -> Vec<String> {
    line_text.split(';')
        .find(|p| p.starts_with("rr__"))
        .and_then(|part| part.strip_prefix("rr__"))
        .map(|s| {
            s.split('_')
                .filter(|pair| pair.contains('='))
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

fn extract_reg_writes(line_text: &str) -> Vec<String> {
    line_text.split(';')
        .find(|p| p.starts_with("rw__"))
        .and_then(|part| part.strip_prefix("rw__"))
        .map(|s| {
            s.split('_')
                .filter(|pair| pair.contains('='))
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

pub fn test_parse_instruction() {
    // 测试第一条指令
    let line1 = "6d2d750e30;10de30;6b09011f;cmp;w8, w9;rr__w9=0x9fd8dde7;;;rw__w8=0x9fd8dde8;";
    let inst1 = ParsedInstruction::parse(line1).unwrap();

    println!("{:#?}", inst1);  // 使用 #? 替代 ?，会自动缩进

    // 测试mov指令
    let line2 = "6d2d750e38;10de38;529bbd09;mov;w9, #0xdde8;;;;rw__w9=0xdde8;";
    let inst2 = ParsedInstruction::parse(line2).unwrap();

    println!("{:#?}", inst2);  // 使用 #? 替代 ?，会自动缩进

    // 测试ldp指令
    let line2 = "6d2d7511dc;10e1dc;a94723e9;ldp;x9, x8, [sp, #0x70];;mr__6cf01578a0_#0x70;ld__6cf0157910_16;rw__x9=0x6d55940fd0_x8=0x6d5592c060;";
    let inst2 = ParsedInstruction::parse(line2).unwrap();

    println!("{:#?}", inst2);  // 使用 #? 替代 ?，会自动缩进

}


pub fn test_parse_single() {
    let line = "6d2d750e30;10de30;6b09011f;cmp;w8, w9;rr__w9=0x9fd8dde7;;;rw__w8=0x9fd8dde8;";
    let inst = ParsedInstruction::parse_single(line).unwrap();
    println!("{:#?}", inst);  // 使用 #? 替代 ?，会自动缩进
}

//根据首个reg的type判断是否是特殊寄存器
//供algop解析使用