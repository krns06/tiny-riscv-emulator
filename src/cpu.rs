use crate::emulator::Emulator;

#[derive(Debug, PartialEq, PartialOrd)]
pub enum InstClass {
    Jump(bool),
    Atomic,
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

#[derive(Debug, PartialEq, PartialOrd)]
pub enum InstIsa {
    A,
    I,
    M,
    Zifencei,
    Zicsr,
    Invalid,
}

#[derive(Debug)]
pub struct Inst {
    name: String,
    class: InstClass,
    format: InstFormat,
    isa: InstIsa,
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
            isa: InstIsa::Invalid,
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

    pub fn isa(&self) -> &InstIsa {
        &self.isa
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
    ($op:ident, Jump, $isa:ident, $format:ident, $raw:expr) => {
        Inst {
            name: String::from(stringify!($op)),
            isa: InstIsa::$isa,
            class: InstClass::Jump(false),
            format: InstFormat::$format,
            raw: $raw,
        }
    };

    ($op:ident, $class:ident, $isa:ident, $format:ident, $raw:expr) => {
        Inst {
            name: String::from(stringify!($op)),
            isa: InstIsa::$isa,
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

        println!(
            "rv64 op: 0b{:07b} funct3: 0x{:x} inst[27:31]: 0b{:05b}",
            op,
            funct3,
            raw_inst >> 27
        );

        match op {
            0b0000011 => match funct3 {
                0b000 => inst!(lb, Load, I, I, raw_inst),
                0b001 => inst!(lh, Load, I, I, raw_inst),
                0b010 => inst!(lw, Load, I, I, raw_inst),
                0b011 => inst!(ld, Load, I, I, raw_inst),
                0b100 => inst!(lbu, Load, I, I, raw_inst),
                0b101 => inst!(lhu, Load, I, I, raw_inst),
                0b110 => inst!(lwu, Load, I, I, raw_inst),
                _ => unimplemented!(),
            },
            0b0001111 => inst!(fence, System, Zifencei, Other, raw_inst),
            0b0010011 => match (funct3, raw_inst >> 26) {
                (0b000, _) => inst!(addi, Alu, I, I, raw_inst),
                (0b001, 0b000000) => inst!(slli, Alu, I, I, raw_inst),
                (0b010, _) => inst!(slti, Alu, I, I, raw_inst),
                (0b011, _) => inst!(sltiu, Alu, I, I, raw_inst),
                (0b100, _) => inst!(xori, Alu, I, I, raw_inst),
                (0b101, 0b000000) => inst!(srli, Alu, I, I, raw_inst),
                (0b101, 0b010000) => inst!(srai, Alu, I, I, raw_inst),
                (0b110, _) => inst!(ori, Alu, I, I, raw_inst),
                (0b111, _) => inst!(andi, Alu, I, I, raw_inst),
                _ => unimplemented!(),
            },
            0b0010111 => inst!(auipc, Alu, I, U, raw_inst),
            0b0011011 => match (funct3, raw_inst >> 26) {
                (0b000, _) => inst!(addiw, Alu, I, I, raw_inst),
                (0b001, 0) => inst!(slliw, Alu, I, I, raw_inst),
                (0b101, 0) => inst!(srliw, Alu, I, I, raw_inst),
                (0b101, 0b010000) => inst!(sraiw, Alu, I, I, raw_inst),
                _ => unimplemented!(),
            },
            0b0100011 => match funct3 {
                0b000 => inst!(sb, Store, I, S, raw_inst),
                0b001 => inst!(sh, Store, I, S, raw_inst),
                0b010 => inst!(sw, Store, I, S, raw_inst),
                0b011 => inst!(sd, Store, I, S, raw_inst),
                _ => unimplemented!(),
            },
            0b0101111 => match (funct3, raw_inst >> 27) {
                (0b010, 0) => inst!(amoadd_w, Atomic, A, R, raw_inst),
                (0b010, 0b00001) => inst!(amoswap_w, Atomic, A, R, raw_inst),
                (0b010, 0b00010) => inst!(lr_w, Atomic, A, R, raw_inst),
                (0b010, 0b00011) => inst!(sc_w, Atomic, A, R, raw_inst),
                (0b010, 0b00100) => inst!(amoxor_w, Atomic, A, R, raw_inst),
                (0b010, 0b01000) => inst!(amoor_w, Atomic, A, R, raw_inst),
                (0b010, 0b01100) => inst!(amoand_w, Atomic, A, R, raw_inst),
                (0b010, 0b10000) => inst!(amomin_w, Atomic, A, R, raw_inst),
                (0b010, 0b10100) => inst!(amomax_w, Atomic, A, R, raw_inst),
                (0b010, 0b11000) => inst!(amominu_w, Atomic, A, R, raw_inst),
                (0b010, 0b11100) => inst!(amomaxu_w, Atomic, A, R, raw_inst),
                (0b011, 0) => inst!(amoadd_d, Atomic, A, R, raw_inst),
                (0b011, 0b00001) => inst!(amoswap_d, Atomic, A, R, raw_inst),
                (0b011, 0b00100) => inst!(amoxor_d, Atomic, A, R, raw_inst),
                (0b011, 0b01000) => inst!(amoor_d, Atomic, A, R, raw_inst),
                (0b011, 0b01100) => inst!(amoand_d, Atomic, A, R, raw_inst),
                (0b011, 0b10000) => inst!(amomin_d, Atomic, A, R, raw_inst),
                (0b011, 0b10100) => inst!(amomax_d, Atomic, A, R, raw_inst),
                (0b011, 0b11000) => inst!(amominu_d, Atomic, A, R, raw_inst),
                (0b011, 0b11100) => inst!(amomaxu_d, Atomic, A, R, raw_inst),
                _ => unimplemented!(),
            },
            0b0110011 => match (funct3, raw_inst >> 25) {
                (0, 0) => inst!(add, Alu, I, R, raw_inst),
                (0, 0b0000001) => inst!(mul, Alu, M, R, raw_inst),
                (0, 0b0100000) => inst!(sub, Alu, I, R, raw_inst),
                (0b001, 0) => inst!(sll, Alu, I, R, raw_inst),
                (0b001, 0b0000001) => inst!(mulh, Alu, M, R, raw_inst),
                (0b010, 0) => inst!(slt, Alu, I, R, raw_inst),
                (0b010, 0b0000001) => inst!(mulhsu, Alu, M, R, raw_inst),
                (0b011, 0) => inst!(sltu, Alu, I, R, raw_inst),
                (0b011, 0b0000001) => inst!(mulhu, Alu, M, R, raw_inst),
                (0b100, 0) => inst!(xor, Alu, I, R, raw_inst),
                (0b100, 0b0000001) => inst!(div, Alu, M, R, raw_inst),
                (0b110, 0) => inst!(or, Alu, I, R, raw_inst),
                (0b110, 0b0000001) => inst!(rem, Alu, M, R, raw_inst),
                (0b101, 0) => inst!(srl, Alu, I, R, raw_inst),
                (0b101, 0b0000001) => inst!(divu, Alu, M, R, raw_inst),
                (0b101, 0b0100000) => inst!(sra, Alu, I, R, raw_inst),
                (0b111, 0) => inst!(and, Alu, I, R, raw_inst),
                (0b111, 0b0000001) => inst!(remu, Alu, M, R, raw_inst),
                _ => unimplemented!(),
            },
            0b0110111 => inst!(lui, Load, I, U, raw_inst),
            0b0111011 => match (funct3, raw_inst >> 25) {
                (0, 0) => inst!(addw, Alu, I, R, raw_inst),
                (0, 0b0000001) => inst!(mulw, Alu, M, R, raw_inst),
                (0, 0b0100000) => inst!(subw, Alu, I, R, raw_inst),
                (0b001, 0) => inst!(sllw, Alu, I, R, raw_inst),
                (0b100, 0b0000001) => inst!(divw, Alu, M, R, raw_inst),
                (0b101, 0) => inst!(srlw, Alu, I, R, raw_inst),
                (0b101, 0b0000001) => inst!(divuw, Alu, M, R, raw_inst),
                (0b101, 0b0100000) => inst!(sraw, Alu, I, R, raw_inst),
                (0b110, 0b0000001) => inst!(remw, Alu, M, R, raw_inst),
                (0b111, 0b0000001) => inst!(remuw, Alu, M, R, raw_inst),
                _ => unimplemented!(),
            },
            0b1100011 => match funct3 {
                0b000 => inst!(beq, Jump, I, B, raw_inst),
                0b001 => inst!(bne, Jump, I, B, raw_inst),
                0b101 => inst!(bge, Jump, I, B, raw_inst),
                0b100 => inst!(blt, Jump, I, B, raw_inst),
                0b110 => inst!(bltu, Jump, I, B, raw_inst),
                0b111 => inst!(bgeu, Jump, I, B, raw_inst),
                _ => unimplemented!(),
            },
            0b1100111 => inst!(jalr, Jump, I, I, raw_inst),
            0b1101111 => inst!(jal, Jump, I, J, raw_inst),
            0b1110011 => match funct3 {
                0b000 => match raw_inst {
                    0x00000073 => inst!(ecall, System, I, Other, raw_inst),
                    0x30200073 => inst!(mret, System, I, Other, raw_inst),
                    _ => unimplemented!(),
                },
                0b001 => inst!(csrrw, Csr, Zicsr, I, raw_inst),
                0b010 => inst!(csrrs, Csr, Zicsr, I, raw_inst),
                0b101 => inst!(csrrwi, Csr, Zicsr, I, raw_inst),
                _ => unimplemented!(),
            },
            _ => unimplemented!("rv64 op: 0b{:07b} funct3: 0x{:x}", op, funct3),
        }
    }
}
