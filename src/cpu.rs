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
    Ca,
    Cb,
    Cj,
    Ci,
    Ciw,
    Cl,
    Cr,
    Cs,
    Css,
    Other,
}

#[derive(Debug, PartialEq, PartialOrd)]
pub enum InstIsa {
    A,
    I,
    M,
    C,
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
    fn c_decode(&self, raw_inst: u32) -> Inst {
        let raw_inst = raw_inst & 0xffff;

        let op = raw_inst & 0x3;

        match (op, raw_inst >> 13) {
            (0, 0) => inst!(c_addi4spn, Alu, C, Ciw, raw_inst),
            (0, 0b010) => inst!(c_lw, Load, C, Cl, raw_inst),
            (0, 0b011) => inst!(c_ld, Load, C, Cl, raw_inst),
            (0, 0b110) => inst!(c_sw, Store, C, Cs, raw_inst),
            (0, 0b111) => inst!(c_sd, Store, C, Cs, raw_inst),
            (0b01, 0b000) if (raw_inst >> 7) & 0x1f == 0 => inst!(c_nop, Alu, C, Ci, raw_inst),
            (0b01, 0b000) => inst!(c_addi, Alu, C, Ci, raw_inst),
            (0b01, 0b001) => inst!(c_addiw, Alu, C, Ci, raw_inst),
            (0b01, 0b010) => inst!(c_li, Load, C, Ci, raw_inst),
            (0b01, 0b011) if (raw_inst >> 7) & 0x1f == 0 => Inst::invalid(), // reserved
            (0b01, 0b011) if (raw_inst >> 7) & 0x1f == 2 => inst!(c_addi16sp, Alu, C, Ci, raw_inst),
            (0b01, 0b011) => inst!(c_lui, Load, C, Ci, raw_inst), // reserved
            (0b01, 0b100) => match (raw_inst >> 10) & 0x3 {
                0b00 => inst!(c_srli, Alu, C, Cb, raw_inst),
                0b01 => inst!(c_srai, Alu, C, Cb, raw_inst),
                0b10 => inst!(c_andi, Alu, C, Cb, raw_inst),
                0b11 => match ((raw_inst >> 12) & 0x1, (raw_inst >> 5) & 0x3) {
                    (0, 0) => inst!(c_sub, Alu, C, Ca, raw_inst),
                    (0, 0b01) => inst!(c_xor, Alu, C, Ca, raw_inst),
                    (0, 0b10) => inst!(c_or, Alu, C, Ca, raw_inst),
                    (0, 0b11) => inst!(c_and, Alu, C, Ca, raw_inst),
                    (1, 0b00) => inst!(c_subw, Alu, C, Ca, raw_inst),
                    (1, 0b01) => inst!(c_addw, Alu, C, Ca, raw_inst),
                    _ => unimplemented!(),
                },
                _ => unimplemented!(),
            },
            (0b01, 0b101) => inst!(c_j, Jump, C, Cj, raw_inst),
            (0b01, 0b110) => inst!(c_beqz, Jump, C, Cb, raw_inst),
            (0b01, 0b111) => inst!(c_bnez, Jump, C, Cb, raw_inst),
            (0b10, 0) => inst!(c_slli, Alu, C, Ci, raw_inst),
            (0b10, 0b010) => inst!(c_lwsp, Load, C, Ci, raw_inst),
            (0b10, 0b011) => inst!(c_ldsp, Load, C, Ci, raw_inst),
            (0b10, 0b100) if raw_inst == 0x9002 => inst!(c_ebreak, System, C, Other, raw_inst),
            (0b10, 0b100) => match ((raw_inst >> 12) & 0x1, (raw_inst >> 2) & 0x1f) {
                (0, 0) => inst!(c_jr, Jump, C, Cr, raw_inst),
                (0, _) => inst!(c_mv, Alu, C, Cr, raw_inst),
                (1, 0) => inst!(c_jalr, Jump, C, Cr, raw_inst),
                (1, _) => inst!(c_add, Alu, C, Cr, raw_inst),
                _ => unimplemented!(),
            },
            (0b10, 0b110) => inst!(c_swsp, Store, C, Css, raw_inst),
            (0b10, 0b111) => inst!(c_sdsp, Store, C, Css, raw_inst),
            _ => unimplemented!(),
        }
    }

    pub(crate) fn decode(&self, raw_inst: u32) -> Inst {
        if raw_inst == 0 {
            return Inst::invalid();
        }

        let op = raw_inst & 0x7f;
        let funct3 = (raw_inst >> 12) & 0x7;

        if op & 0x3 < 3 {
            return self.c_decode(raw_inst);
        }

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
                0b000 => match raw_inst >> 25 {
                    0b0001001 => inst!(sfence_vma, System, Zifencei, R, raw_inst),
                    _ => match raw_inst {
                        0x00000073 => inst!(ecall, System, I, Other, raw_inst),
                        0x10200073 => inst!(sret, System, I, Other, raw_inst),
                        0x30200073 => inst!(mret, System, I, Other, raw_inst),
                        0x10500073 => inst!(wfi, System, I, Other, raw_inst),
                        _ => unimplemented!(),
                    },
                },
                0b001 => inst!(csrrw, Csr, Zicsr, I, raw_inst),
                0b010 => inst!(csrrs, Csr, Zicsr, I, raw_inst),
                0b011 => inst!(csrrc, Csr, Zicsr, I, raw_inst),
                0b101 => inst!(csrrwi, Csr, Zicsr, I, raw_inst),
                0b110 => inst!(csrrsi, Csr, Zicsr, I, raw_inst),
                0b111 => inst!(csrrci, Csr, Zicsr, I, raw_inst),
                _ => unimplemented!(),
            },
            _ => unimplemented!("rv64 op: 0b{:07b} funct3: 0x{:x}", op, funct3),
        }
    }
}
