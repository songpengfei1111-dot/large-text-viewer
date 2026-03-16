use csv::ReaderBuilder;
use std::collections::{HashMap, HashSet};
use std::path::Path;

const SEP: char = ';';

#[derive(Debug, Clone)]
pub struct AssemblyInstruction {
    pub full_addr: String,
    pub offset: String,
    pub hex: String,
    pub opcode: String,
    pub operands: String,
    pub read_regs: String,
    pub mem_detail: String,
    pub mem_info: String,
    pub write_regs: String,
    pub unknown: String,
}

impl AssemblyInstruction {
    pub fn from_line(line: &str) -> Self {
        let parts: Vec<&str> = line.split(SEP).collect();
        Self::from_parts(&parts)
    }

    pub fn from_csv_record(record: csv::StringRecord) -> Self {
        let parts: Vec<&str> = (0..10)
            .map(|i| record.get(i).unwrap_or(""))
            .collect();
        Self::from_parts(&parts)
    }

    //字段赋值
    fn from_parts(parts: &[&str]) -> Self {
        AssemblyInstruction {
            full_addr: parts.get(0).unwrap_or(&"").to_string(),
            offset: parts.get(1).unwrap_or(&"").to_string(),
            hex: parts.get(2).unwrap_or(&"").to_string(),
            opcode: parts.get(3).unwrap_or(&"").to_string(),
            operands: parts.get(4).unwrap_or(&"").to_string(),
            read_regs: parts.get(5).unwrap_or(&"").to_string(),
            mem_detail: parts.get(6).unwrap_or(&"").to_string(),
            mem_info: parts.get(7).unwrap_or(&"").to_string(),
            write_regs: parts.get(8).unwrap_or(&"").to_string(),
            unknown: parts.get(9).unwrap_or(&"").to_string(),
        }
    }

    pub fn to_parts(&self) -> Vec<&str> {
        vec![
            &self.full_addr,
            &self.offset,
            &self.hex,
            &self.opcode,
            &self.operands,
            &self.read_regs,
            &self.mem_detail,
            &self.mem_info,
            &self.write_regs,
            &self.unknown,
        ]
    }

    pub fn get_lr_value(&self) -> Option<u64> {
        if self.write_regs.contains("lr=") {
            if let Some(start) = self.write_regs.find("lr=") {
                let mut lr_part = &self.write_regs[start + 3..];
                if lr_part.starts_with("0x") {
                    lr_part = &lr_part[2..];
                }
                if let Some(end) = lr_part.find(|c: char| !c.is_ascii_hexdigit()) {
                    let lr_str = &lr_part[..end];
                    if let Ok(lr_val) = u64::from_str_radix(lr_str, 16) {
                        return Some(lr_val);
                    }
                } else {
                    if let Ok(lr_val) = u64::from_str_radix(lr_part, 16) {
                        return Some(lr_val);
                    }
                }
            }
        }
        None
    }
}

pub struct AssemblyAnalyzer {
    instructions: Vec<AssemblyInstruction>,
}

impl AssemblyAnalyzer {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let mut rdr = ReaderBuilder::new()
            .has_headers(false)
            .delimiter(b';')
            .flexible(false)
            .from_path(path)?;
        
        let instructions = rdr
            .records()
            .map(|result| result.map(AssemblyInstruction::from_csv_record))
            .collect::<Result<Vec<_>, _>>()?;
        
        Ok(Self { instructions })
    }

    pub fn instructions(&self) -> &[AssemblyInstruction] {
        &self.instructions
    }

    pub fn get_total_instructions(&self) -> usize {
        self.instructions.len()
    }

    pub fn get_opcode_frequency(&self, n: usize) -> Vec<(String, usize)> {
        let mut freq = HashMap::new();
        for instr in &self.instructions {
            *freq.entry(instr.opcode.clone()).or_insert(0) += 1;
        }
        
        let mut vec: Vec<_> = freq.into_iter().collect();
        vec.sort_by(|a, b| b.1.cmp(&a.1));
        vec.truncate(n);
        vec
    }

    pub fn get_unique_opcodes(&self) -> Vec<String> {
        let mut opcodes: Vec<_> = self.instructions
            .iter()
            .map(|instr| instr.opcode.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        opcodes.sort();
        opcodes
    }

    pub fn get_opcode_count(&self) -> usize {
        self.instructions
            .iter()
            .map(|instr| &instr.opcode)
            .collect::<HashSet<_>>()
            .len()
    }

    pub fn filter_by_opcode(&self, opcode: &str) -> Vec<&AssemblyInstruction> {
        self.instructions
            .iter()
            .filter(|instr| instr.opcode == opcode)
            .collect()
    }

    pub fn get_memory_operations(&self) -> Vec<&AssemblyInstruction> {
        let memory_ops = vec!["ldr", "str", "stp", "ldp", "ldrb", "strb", "stur", "ldur"];
        self.instructions
            .iter()
            .filter(|instr| memory_ops.contains(&instr.opcode.as_str()))
            .collect()
    }

    pub fn get_branch_operations(&self) -> Vec<&AssemblyInstruction> {
        let branch_ops = vec!["b", "bl", "cbz", "cbnz", "tbz", "tbnz", "b.eq", "b.ne", "b.lt", "b.le", "b.gt", "b.ge", "b.hi", "b.hs", "b.lo", "b.ls"];
        self.instructions
            .iter()
            .filter(|instr| branch_ops.contains(&instr.opcode.as_str()))
            .collect()
    }

    pub fn get_operation_distribution(&self) -> Vec<(String, usize, f64)> {
        let total = self.instructions.len() as f64;
        let freq = self.get_opcode_frequency(usize::MAX);
        freq.into_iter()
            .map(|(opcode, count)| (opcode, count, (count as f64 / total) * 100.0))
            .collect()
    }
}
