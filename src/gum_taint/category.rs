pub const REG_INVALID: usize = 255;
pub const REG_SP: usize = 31;
pub const REG_XZR: usize = 32;
pub const REG_NZCV: usize = 33;

pub fn parse_reg_name(s: &str) -> usize {
    let s = s.trim();
    if s.is_empty() { return REG_INVALID; }
    
    let bytes = s.as_bytes();
    let first = bytes[0].to_ascii_lowercase();
    
    match first {
        b'x' | b'w' => {
            if bytes.len() == 3 && s.eq_ignore_ascii_case("wzr") { return REG_XZR; }
            if bytes.len() == 3 && s.eq_ignore_ascii_case("xzr") { return REG_XZR; }
            if let Ok(n) = s[1..].parse::<usize>() {
                if n <= 30 { return n; }
            }
        },
        b's' => {
            if s.eq_ignore_ascii_case("sp") { return REG_SP; }
            if let Ok(n) = s[1..].parse::<usize>() {
                if n <= 31 { return 64 + n; } // 映射到 Q 系列
            }
        },
        b'f' => {
            if s.eq_ignore_ascii_case("fp") { return 29; } // x29
        },
        b'l' => {
            if s.eq_ignore_ascii_case("lr") { return 30; } // x30
        },
        b'q' | b'd' | b'h' | b'b' | b'v' => {
            if let Ok(n) = s[1..].parse::<usize>() {
                if n <= 31 { return 64 + n; }
            }
        },
        b'n' => {
            if s.eq_ignore_ascii_case("nzcv") { return REG_NZCV; }
        },
        _ => {}
    }
    
    REG_INVALID
}

pub fn reg_name(id: usize) -> String {
    match id {
        0..=30 => format!("x{}", id),
        31 => "sp".to_string(),
        32 => "xzr".to_string(),
        33 => "nzcv".to_string(),
        64..=95 => format!("q{}", id - 64),
        _ => "?".to_string(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsnCategory {
    DataMove,
    ImmLoad,
    PartialModify,
    Arithmetic,
    Logic,
    ShiftExt,
    Bitfield,
    Load,
    Store,
    Compare,
    CondSelect,
    Branch,
    Other,
}

pub fn classify_mnemonic(m: &str) -> InsnCategory {
    let m = m.to_ascii_lowercase();
    let m = m.as_str();
    
    match m {
        "mov" | "mvn" => InsnCategory::DataMove,
        "movz" | "movn" => InsnCategory::ImmLoad,
        "movk" => InsnCategory::PartialModify,
        "mul" | "madd" | "msub" | "mneg" => InsnCategory::Arithmetic,
        
        "neg" | "negs" | "ngc" | "ngcs" => InsnCategory::DataMove,
        "nop" => InsnCategory::Branch,
        
        "cls" | "clz" => InsnCategory::DataMove,
        "cmp" | "cmn" | "ccmp" | "ccmn" => InsnCategory::Compare,
        "csel" | "csinc" | "csinv" | "csneg" | "cset" | "csetm" | "cinc" | "cinv" | "cneg" => InsnCategory::CondSelect,
        "cbz" | "cbnz" => InsnCategory::Branch,
        
        "add" | "adds" | "adc" | "adcs" => InsnCategory::Arithmetic,
        "and" | "ands" => InsnCategory::Logic,
        "adr" | "adrp" => InsnCategory::ImmLoad,
        "asr" => InsnCategory::ShiftExt,
        s if s.starts_with("aut") => InsnCategory::Branch,
        
        "sub" | "subs" | "sbc" | "sbcs" | "sdiv" | "smull" | "smulh" | "smaddl" | "smsubl" => InsnCategory::Arithmetic,
        "str" | "strb" | "strh" | "stur" | "sturb" | "sturh" | "stp" | "stlr" | "stlrb" | "stlrh" | "stxr" | "stlxr" | "stxrb" | "stlxrb" | "stxrh" | "stlxrh" | "stxp" | "stlxp" => InsnCategory::Store,
        "sbfm" | "sbfx" => InsnCategory::Bitfield,
        "sxtb" | "sxth" | "sxtw" => InsnCategory::ShiftExt,
        "scvtf" => InsnCategory::DataMove,
        "svc" => InsnCategory::Branch,
        
        "ldr" | "ldrb" | "ldrh" | "ldrsw" | "ldrsb" | "ldrsh" | "ldur" | "ldurb" | "ldurh" | "ldursw" | "ldursb" | "ldursh" | "ldp" | "ldpsw" | "ldar" | "ldarb" | "ldarh" | "ldxr" | "ldaxr" | "ldxrb" | "ldaxrb" | "ldxrh" | "ldaxrh" | "ldxp" | "ldaxp" | "ldnp" | "ldtr" | "ldtrb" | "ldtrh" | "ldtrsw" | "ldtrsb" | "ldtrsh" => InsnCategory::Load,
        "lsl" | "lsr" => InsnCategory::ShiftExt,
        
        "dmb" | "dsb" | "dc" => InsnCategory::Branch,
        
        "orr" | "orn" => InsnCategory::Logic,
        
        "eor" | "eon" => InsnCategory::Logic,
        "extr" => InsnCategory::Bitfield,
        
        "fmov" => InsnCategory::DataMove,
        "fadd" | "fsub" | "fmul" | "fdiv" | "fneg" | "fabs" | "fsqrt" | "fmadd" | "fmsub" | "fnmadd" | "fnmsub" | "fmin" | "fmax" | "fnmul" => InsnCategory::Arithmetic,
        "fcmp" | "fccmp" | "fcmpe" | "fccmpe" => InsnCategory::Compare,
        "fcsel" => InsnCategory::CondSelect,
        s if s.starts_with("fcvt") || s.starts_with("frint") => InsnCategory::DataMove,
        
        "isb" | "ic" => InsnCategory::Branch,
        
        "bic" | "bics" => InsnCategory::Logic,
        "bfm" | "bfi" | "bfxil" => InsnCategory::Bitfield,
        "b" | "bl" | "br" | "blr" | "bti" => InsnCategory::Branch,
        s if s.starts_with("b.") => InsnCategory::Branch,
        
        "ret" | "retaa" | "retab" => InsnCategory::Branch,
        "rbit" | "rev" | "rev16" | "rev32" | "rev64" => InsnCategory::DataMove,
        "ror" => InsnCategory::ShiftExt,
        
        "udiv" | "umull" | "umulh" | "umaddl" | "umsubl" => InsnCategory::Arithmetic,
        "ubfm" | "ubfx" => InsnCategory::Bitfield,
        "uxtb" | "uxth" => InsnCategory::ShiftExt,
        "ucvtf" => InsnCategory::DataMove,
        
        "tst" => InsnCategory::Compare,
        "tbz" | "tbnz" => InsnCategory::Branch,
        
        "paciasp" | "pacibsp" | "pacia" | "pacib" | "pacda" | "pacdb" => InsnCategory::Branch,
        "prfm" | "prfum" => InsnCategory::Branch,
        
        _ => InsnCategory::Other,
    }
}
