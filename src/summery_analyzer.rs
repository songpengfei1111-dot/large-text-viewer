use csv::ReaderBuilder;
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct AssemblyInstruction {
    pub full_addr: String,
    pub offset: String,
    pub hex: String,
    pub opcode: String,
    pub operands: String,
    pub read_regs: String,
    pub mem_read: String,
    pub mem_write: String,
    pub write_regs: String,
    pub unknown: String,
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
        
        let mut instructions = Vec::new();
        
        for result in rdr.records() {
            let record = result?;
            instructions.push(AssemblyInstruction {
                full_addr: record.get(0).unwrap_or("").to_string(),
                offset: record.get(1).unwrap_or("").to_string(),
                hex: record.get(2).unwrap_or("").to_string(),
                opcode: record.get(3).unwrap_or("").to_string(),
                operands: record.get(4).unwrap_or("").to_string(),
                read_regs: record.get(5).unwrap_or("").to_string(),
                mem_read: record.get(6).unwrap_or("").to_string(),
                mem_write: record.get(7).unwrap_or("").to_string(),
                write_regs: record.get(8).unwrap_or("").to_string(),
                unknown: record.get(9).unwrap_or("").to_string(),
            });
        }
        
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
        let mut set = HashSet::new();
        for instr in &self.instructions {
            set.insert(instr.opcode.clone());
        }
        let mut vec: Vec<_> = set.into_iter().collect();
        vec.sort();
        vec
    }

    pub fn get_opcode_count(&self) -> usize {
        self.get_unique_opcodes().len()
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
