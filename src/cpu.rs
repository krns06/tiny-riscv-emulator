use crate::emulator::Emulator;

#[derive(Debug, PartialEq, PartialOrd)]
pub enum InstClass {
    Jump(bool),
    Alu,
    Csr,
    Load,
    Store,
    System,
    Invalid,
}

#[derive(Debug)]
pub enum InstFormat {
    B,
    I,
    J,
    R,
    S,
    U,
    Other,
}

#[derive(Debug)]
pub struct Inst {
    name: String,
    class: InstClass,
    format: InstFormat,
    raw: u32,
}

impl Default for Inst {
    fn default() -> Self {
        Self::invalid()
    }
}

impl Inst {
    fn invalid() -> Self {
        Self {
            name: String::new(),
            class: InstClass::Invalid,
            format: InstFormat::Other,
            raw: 0,
        }
    }
}

impl Inst {
    pub fn is_valid(&self) -> bool {
        self.class != InstClass::Invalid
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn class(&self) -> &InstClass {
        &self.class
    }

    pub fn set_class(&mut self, class: InstClass) {
        self.class = class;
    }

    pub fn format(&self) -> &InstFormat {
        &self.format
    }

    pub fn raw(&self) -> u32 {
        self.raw
    }

    pub fn op(&self) -> u32 {
        self.raw & 0x7f
    }
}

macro_rules! inst {
    ($op:ident, Jump, $format:ident, $raw:expr) => {
        Inst {
            name: String::from(stringify!($op)),
            class: InstClass::Jump(false),
            format: InstFormat::$format,
            raw: $raw,
        }
    };

    ($op:ident, $class:ident, $format:ident, $raw:expr) => {
        Inst {
            name: String::from(stringify!($op)),
            class: InstClass::$class,
            format: InstFormat::$format,
            raw: $raw,
        }
    };
}

impl Emulator {
    pub(crate) fn decode(&self, raw_inst: u32) -> Inst {
        if self.is_c_extension_enabled() && raw_inst & 0x3 < 3 {
            // C拡張はまだ実装しない
            unimplemented!();
        }

        // instruction & 0x3 != 3以外ならRV32もしくはRV64ではない可能性がある。
        if raw_inst & 0x3 != 3 {
            return Inst::invalid();
        }

        let op = raw_inst & 0x7f;
        let funct3 = (raw_inst >> 12) & 0x7;

        println!("rv64 op: 0b{:07b} funct3: 0x{:x}", op, funct3);

        match op {
            0b0000011 => match funct3 {
                0b000 => inst!(lb, Load, I, raw_inst),
                0b001 => inst!(lh, Load, I, raw_inst),
                0b010 => inst!(lw, Load, I, raw_inst),
                0b011 => inst!(ld, Load, I, raw_inst),
                0b100 => inst!(lbu, Load, I, raw_inst),
                0b101 => inst!(lhu, Load, I, raw_inst),
                0b110 => inst!(lwu, Load, I, raw_inst),
                _ => unimplemented!(),
            },
            0b0001111 => inst!(fence, System, Other, raw_inst),
            0b0010011 => match (funct3, raw_inst >> 26) {
                (0b000, _) => inst!(addi, Alu, I, raw_inst),
                (0b001, 0b000000) => inst!(slli, Alu, I, raw_inst),
                (0b010, _) => inst!(slti, Alu, I, raw_inst),
                (0b011, _) => inst!(sltiu, Alu, I, raw_inst),
                (0b100, _) => inst!(xori, Alu, I, raw_inst),
                (0b101, 0b000000) => inst!(srli, Alu, I, raw_inst),
                (0b101, 0b010000) => inst!(srai, Alu, I, raw_inst),
                (0b110, _) => inst!(ori, Alu, I, raw_inst),
                (0b111, _) => inst!(andi, Alu, I, raw_inst),
                _ => unimplemented!(),
            },
            0b0010111 => inst!(auipc, Alu, U, raw_inst),
            0b0011011 => match (funct3, raw_inst >> 26) {
                (0b000, _) => inst!(addiw, Alu, I, raw_inst),
                (0b001, 0) => inst!(slliw, Alu, I, raw_inst),
                (0b101, 0) => inst!(srliw, Alu, I, raw_inst),
                (0b101, 0b010000) => inst!(sraiw, Alu, I, raw_inst),
                _ => unimplemented!(),
            },
            0b0100011 => match funct3 {
                0b000 => inst!(sb, Store, S, raw_inst),
                0b001 => inst!(sh, Store, S, raw_inst),
                0b010 => inst!(sw, Store, S, raw_inst),
                0b011 => inst!(sd, Store, S, raw_inst),
                _ => unimplemented!(),
            },
            0b0110011 => match (funct3, raw_inst >> 25) {
                (0, 0) => inst!(add, Alu, R, raw_inst),
                (0, 0b0000001) => inst!(mul, Alu, R, raw_inst),
                (0, 0b0100000) => inst!(sub, Alu, R, raw_inst),
                (0b001, 0) => inst!(sll, Alu, R, raw_inst),
                (0b010, 0) => inst!(slt, Alu, R, raw_inst),
                (0b011, 0) => inst!(sltu, Alu, R, raw_inst),
                (0b100, 0) => inst!(xor, Alu, R, raw_inst),
                (0b110, 0) => inst!(or, Alu, R, raw_inst),
                (0b101, 0) => inst!(srl, Alu, R, raw_inst),
                (0b101, 0b0100000) => inst!(sra, Alu, R, raw_inst),
                (0b111, 0) => inst!(and, Alu, R, raw_inst),
                _ => unimplemented!(),
            },
            0b0110111 => inst!(lui, Load, U, raw_inst),
            0b0111011 => match (funct3, raw_inst >> 25) {
                (0, 0) => inst!(addw, Alu, R, raw_inst),
                (0, 0b0100000) => inst!(subw, Alu, R, raw_inst),
                (0b001, 0) => inst!(sllw, Alu, R, raw_inst),
                (0b101, 0) => inst!(srlw, Alu, R, raw_inst),
                (0b101, 0b0100000) => inst!(sraw, Alu, R, raw_inst),
                _ => unimplemented!(),
            },
            0b1100011 => match funct3 {
                0b000 => inst!(beq, Jump, B, raw_inst),
                0b001 => inst!(bne, Jump, B, raw_inst),
                0b101 => inst!(bge, Jump, B, raw_inst),
                0b100 => inst!(blt, Jump, B, raw_inst),
                0b110 => inst!(bltu, Jump, B, raw_inst),
                0b111 => inst!(bgeu, Jump, B, raw_inst),
                _ => unimplemented!(),
            },
            0b1100111 => inst!(jalr, Jump, I, raw_inst),
            0b1101111 => inst!(jal, Jump, J, raw_inst),
            0b1110011 => match funct3 {
                0b000 => match raw_inst {
                    0x00000073 => inst!(ecall, System, Other, raw_inst),
                    0x30200073 => inst!(mret, System, Other, raw_inst),
                    _ => unimplemented!(),
                },
                0b001 => inst!(csrrw, Csr, I, raw_inst),
                0b010 => inst!(csrrs, Csr, I, raw_inst),
                0b101 => inst!(csrrwi, Csr, I, raw_inst),
                _ => unimplemented!(),
            },
            _ => unimplemented!("rv64 op: 0b{:07b} funct3: 0x{:x}", op, funct3),
        }
    }
}
