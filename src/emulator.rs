use std::{error::Error, path::Path};

use crate::{
    cpu::{Inst, InstClass, InstIsa},
    csr::{
        Csr, CSR_MCAUSE, CSR_MEDELEG, CSR_MEPC, CSR_MIDELEG, CSR_MIE, CSR_MIP, CSR_MISA,
        CSR_MSTATUS, CSR_MSTATUS_MIE_MASK, CSR_MSTATUS_MPIE_MASK, CSR_MSTATUS_MPP_MASK,
        CSR_MSTATUS_SIE_MASK, CSR_MSTATUS_SPIE_MASK, CSR_MSTATUS_SPP_MASK, CSR_MSTATUS_TSR_MASK,
        CSR_MSTATUS_TW_MASK, CSR_MTVAL, CSR_MTVEC, CSR_SCAUSE, CSR_SEPC, CSR_SSTATUS,
        CSR_SSTATUS_MASK, CSR_STVAL, CSR_STVEC,
    },
    exception::Exception::{self, *},
    memory::Memory,
    register::Register,
    Priv, Result,
};

// 現在は1M byte
const MEMORY_SIZE: usize = 1024 * 1024;

// 符号拡張する関数
// bitで符号に相当するビットを指定する。０インデックスである。
// bitを64より大きい値を指定するとオーバーフローする。
// 指定したbit以上の値を与えてはいけない。
fn sign_extend(bit: u8, v: u64) -> u64 {
    let mask = (u64::MAX >> 1) ^ (2u64.pow(bit as u32) - 1);

    (mask + v) ^ mask
}

fn sign_extend_128bit(bit: u8, v: u128) -> u128 {
    let mask = (u128::MAX >> 1) ^ (2u128.pow(bit as u32) - 1);

    (mask + v) ^ mask
}

fn extract_r_type(instruction: u32) -> (u8, u8, u8, u8) {
    let rd = (instruction >> 7) & 0x1f;
    let rs1 = (instruction >> 15) & 0x1f;
    let rs2 = (instruction >> 20) & 0x1f;
    let funct7 = instruction >> 25;

    (rd as u8, rs1 as u8, rs2 as u8, funct7 as u8)
}

fn extract_i_type(instruction: u32) -> (u8, u8, u64) {
    let rd = (instruction >> 7) & 0x1f;
    let rs1 = (instruction >> 15) & 0x1f;
    let imm = (instruction >> 20) as u64;

    (rd as u8, rs1 as u8, imm)
}

fn extract_s_type(instruction: u32) -> (u8, u8, u64) {
    let rs1 = (instruction >> 15) & 0x1f;
    let rs2 = (instruction >> 20) & 0x1f;
    let imm = ((instruction & 0xfe000000) >> 20) | ((instruction & 0xf80) >> 7);

    (rs1 as u8, rs2 as u8, imm as u64)
}

fn extract_b_type(instruction: u32) -> (u8, u8, u64) {
    let rs1 = (instruction >> 15) & 0x1f;
    let rs2 = (instruction >> 20) & 0x1f;
    let imm = ((instruction >> 19) & 0x1000)
        | ((instruction << 4) & 0x800)
        | ((instruction >> 20) & 0x7e0)
        | ((instruction >> 7) & 0x1e);

    (rs1 as u8, rs2 as u8, imm as u64)
}

fn extract_u_type(instruction: u32) -> (u8, u64) {
    let rd = (instruction >> 7) & 0x1f;
    let imm = instruction & 0xfffff000;

    (rd as u8, imm as u64)
}

fn extract_j_type(instruction: u32) -> (u8, u64) {
    let rd = (instruction >> 7) & 0x1f;

    let imm = ((instruction >> 11) & 0x100000)
        | (instruction & 0xff000)
        | ((instruction >> 9) & 0x800)
        | ((instruction >> 20) & 0x7fe);

    (rd as u8, imm as u64)
}

// RVC RegisterからInteger Registerに変換する関数
// 8以上のレジスタを与えられた場合はpanicを起こす。
fn convert_from_c_reg_to_i(c_reg: u16) -> u8 {
    if c_reg > 7 {
        panic!("Error: Invalid RVC Register.");
    }

    c_reg as u8 + 8
}

fn extract_cr_type(instruction: u16) -> (u8, u8) {
    let rs2 = (instruction >> 2) & 0x1f;
    let rd = (instruction >> 7) & 0x1f;

    (rs2 as u8, rd as u8)
}

fn extract_ci_type(instruction: u16) -> (u8, u64) {
    let rd = (instruction >> 7) & 0x1f;
    let imm = ((instruction >> 7) & 0x20) | ((instruction >> 2) & 0x1f);

    (rd as u8, imm as u64)
}

fn extract_ciw_type(instruction: u16) -> (u8, u64) {
    let rd = convert_from_c_reg_to_i((instruction >> 2) & 0x7);
    let imm = (instruction >> 5) & 0xff;

    (rd, imm as u64)
}

// CL: (rd, rs1, imm)
// CS: (rs2, rs1, imm)
fn extract_clcs_type(instruction: u16) -> (u8, u8, u64) {
    let rd = convert_from_c_reg_to_i((instruction >> 2) & 0x7);
    let rs1 = convert_from_c_reg_to_i((instruction >> 7) & 0x7);
    let imm = (instruction >> 5) & 0x3;

    (rd, rs1, imm as u64)
}

fn extract_cb_type(instruction: u16) -> (u8, u64) {
    let rs1 = convert_from_c_reg_to_i((instruction >> 7) & 0x7);
    let imm = ((instruction >> 5) & 0xe0) | ((instruction >> 2) & 0x1f);

    (rs1, imm as u64)
}

fn extract_css_type(instruction: u16) -> (u8, u64) {
    let rs2 = (instruction >> 2) & 0x1f;
    let imm = (instruction >> 7) & 0x3f;

    (rs2 as u8, imm as u64)
}

fn calc_c_offset_5_3_2_6(imm: u64) -> u64 {
    ((imm << 6) & 0x40) | ((imm << 1) & 0x38) | ((imm << 1) & 0x4)
}

fn calc_c_offset_5_3_7_6(imm: u64) -> u64 {
    ((imm << 6) & 0xc0) | ((imm << 1) & 0x38)
}

#[derive(Default)]
pub struct Emulator {
    pub(crate) memory: Memory<MEMORY_SIZE>,
    pub(crate) regs: [u64; 31],
    pub(crate) csr: Csr,
    pub(crate) pc: u64,
    pub(crate) current_priv: Priv,
    pub(crate) inst: Inst,
    pub(crate) reserved_memory_ranges: Vec<(usize, usize)>, // 予約されたメモリ領域を指定する。(begin, end)

    pub(crate) riscv_tests_finished: bool, // riscv-testsが終了したかどうかを表すフラグ
    pub(crate) riscv_tests_exit_memory_address: usize, // riscv-testsが終了するメモリアドレス
}

impl Emulator {
    // プログラムをロードする関数
    // 将来的にはロードする位置を指定できるようにしたい。
    // 遅延ロードとかもやってみたい。
    pub fn load<P: AsRef<Path>>(
        &mut self,
        filename: P,
    ) -> core::result::Result<(), Box<dyn Error>> {
        self.initialize_regs();
        self.initialize_csr();

        self.riscv_tests_finished = false;

        self.memory.load(filename)?;

        Ok(())
    }

    fn initialize_regs(&mut self) {
        self.regs = [0; 31];
        self.pc = 0;
    }

    // メモリを読み込むときに使用する関数
    fn read_memory<const SIZE: usize>(&self, address: usize) -> Result<[u8; SIZE]> {
        Ok(self.memory.read::<SIZE>(address))
    }

    // メモリを書き込むときに使用する関数
    fn write_memory(&mut self, address: usize, values: &[u8]) -> Result<()> {
        if address == self.riscv_tests_exit_memory_address {
            self.riscv_tests_finished = true;
        }

        self.memory.write(address, values);

        Ok(())
    }

    // レジスタを読み込むときに使用する関数
    fn read_reg(&self, reg: Register) -> u64 {
        use crate::register::Register::*;

        match reg {
            X(0) => 0,
            X(i) => {
                if i > 31 {
                    panic!("Error: Unknown register x{}.", i);
                } else {
                    self.regs[i as usize - 1]
                }
            }
            Pc => self.pc,
        }
    }

    // レジスタを書き込むときに使用する関数
    fn write_reg(&mut self, reg: Register, value: u64) {
        use crate::register::Register::*;

        match reg {
            X(0) => {}
            X(i) => {
                if i > 31 {
                    panic!("Error: Unknown register x{}.", i);
                } else {
                    self.regs[i as usize - 1] = value;
                }
            }
            Pc => self.pc = value,
        }
    }

    pub(crate) fn check_misaligned_nbyte_misaligned(&self, address: u64, n: u64) -> Result<()> {
        if address % n == 0 {
            Ok(())
        } else {
            Err(InstructionAddressMissaligned)
        }
    }

    // 4byteアライメントを確かめる関数
    // C拡張の場合はミスアライメントの例外は発生しないためOk(())を返す。
    pub(crate) fn check_misaligned(&self, address: u64) -> Result<()> {
        if !self.is_c_extension_enabled() {
            self.check_misaligned_nbyte_misaligned(address, 4)
        } else {
            Ok(())
        }
    }

    // 予約されたメモリ領域を追加する関数
    // LR.D/Wで使用
    // range: (begin, end)
    // 同じ範囲が与えられたらそれを削除してpushする。
    // 一部が被る場合は前に保存していた領域を削除する。
    fn push_reserved_memory_range(&mut self, range: (usize, usize)) {
        self.reserved_memory_ranges
            .retain(|r| range.1 < r.0 || range.0 > r.1);
        self.reserved_memory_ranges.push(range);
    }

    // 予約されたメモリ領域を一つ取り出す関数
    // SC.D/Wで使用
    fn pop_reserved_memory_range(&mut self) -> Option<(usize, usize)> {
        self.reserved_memory_ranges.pop()
    }

    // 命令を取り出す関数
    // run以外から呼んではいけない。
    fn fetch(&mut self) -> u32 {
        u32::from_le_bytes(self.memory.read::<4>(self.pc as usize))
    }

    fn can_exec(&self) -> bool {
        self.inst.is_valid()
            && if self.inst.raw() & 0x3 < 3 {
                self.is_c_extension_enabled()
            } else {
                true
            }
    }

    // 命令を格納するバイト列から実行する命令を判定し命令を実行する関数
    fn exec(&mut self) -> Result<()> {
        if !self.can_exec() {
            return Err(IllegralInstruction);
        }

        use crate::cpu::InstFormat::*;

        let name = self.inst.name();

        match self.inst.format() {
            B => {
                let (rs1, rs2, imm) = extract_b_type(self.inst.raw());
                let offset = sign_extend(12, imm);

                let mut flag = false;

                match name {
                    "beq" => {
                        flag = self.read_reg(Register::X(rs1)) == self.read_reg(Register::X(rs2));
                    }
                    "bne" => {
                        flag = self.read_reg(Register::X(rs1)) != self.read_reg(Register::X(rs2));
                    }
                    "blt" => {
                        flag = self.read_reg(Register::X(rs2)) as i64
                            > self.read_reg(Register::X(rs1)) as i64;
                    }
                    "bge" => {
                        flag = self.read_reg(Register::X(rs1)) as i64
                            >= self.read_reg(Register::X(rs2)) as i64;
                    }
                    "bltu" => {
                        flag = self.read_reg(Register::X(rs2)) > self.read_reg(Register::X(rs1));
                    }
                    "bgeu" => {
                        flag = self.read_reg(Register::X(rs1)) >= self.read_reg(Register::X(rs2));
                    }
                    _ => unimplemented!(),
                }

                if flag {
                    let dst = self.read_reg(Register::Pc).wrapping_add(offset);
                    self.check_misaligned(dst)?;

                    self.inst.set_class(InstClass::Jump(true));
                    self.write_reg(Register::Pc, dst);
                }
            }
            I => {
                let (rd, rs1, imm) = extract_i_type(self.inst.raw());

                match name {
                    "lb" => {
                        let bytes = self.read_memory::<1>(
                            self.read_reg(Register::X(rs1))
                                .wrapping_add(sign_extend(11, imm))
                                as usize,
                        )?;

                        self.write_reg(
                            Register::X(rd),
                            sign_extend(7, u8::from_le_bytes(bytes) as u64),
                        );
                    }
                    "lh" => {
                        let bytes = self.read_memory::<2>(
                            self.read_reg(Register::X(rs1))
                                .wrapping_add(sign_extend(11, imm))
                                as usize,
                        )?;

                        self.write_reg(
                            Register::X(rd),
                            sign_extend(15, u16::from_le_bytes(bytes) as u64),
                        );
                    }
                    "lw" => {
                        let bytes = self.read_memory::<4>(
                            self.read_reg(Register::X(rs1))
                                .wrapping_add(sign_extend(11, imm))
                                as usize,
                        )?;

                        self.write_reg(
                            Register::X(rd),
                            sign_extend(31, u32::from_le_bytes(bytes) as u64),
                        );
                    }
                    "ld" => {
                        let bytes = self.read_memory::<8>(
                            self.read_reg(Register::X(rs1))
                                .wrapping_add(sign_extend(11, imm))
                                as usize,
                        )?;

                        self.write_reg(Register::X(rd), u64::from_le_bytes(bytes));
                    }
                    "lbu" => {
                        let bytes = self.read_memory::<1>(
                            self.read_reg(Register::X(rs1))
                                .wrapping_add(sign_extend(11, imm))
                                as usize,
                        )?;

                        self.write_reg(Register::X(rd), u8::from_le_bytes(bytes) as u64);
                    }
                    "lhu" => {
                        let bytes = self.read_memory::<2>(
                            self.read_reg(Register::X(rs1))
                                .wrapping_add(sign_extend(11, imm))
                                as usize,
                        )?;

                        self.write_reg(Register::X(rd), u16::from_le_bytes(bytes) as u64);
                    }
                    "lwu" => {
                        let bytes = self.read_memory::<4>(
                            self.read_reg(Register::X(rs1))
                                .wrapping_add(sign_extend(11, imm))
                                as usize,
                        )?;

                        self.write_reg(Register::X(rd), u32::from_le_bytes(bytes) as u64);
                    }
                    "addi" => self.write_reg(
                        Register::X(rd),
                        self.read_reg(Register::X(rs1))
                            .wrapping_add(sign_extend(11, imm)),
                    ),
                    "slli" => self.write_reg(
                        Register::X(rd),
                        self.read_reg(Register::X(rs1)) << (imm & 0x3f),
                    ),
                    "slti" => self.write_reg(
                        Register::X(rd),
                        if sign_extend(11, imm) as i64 > self.read_reg(Register::X(rs1)) as i64 {
                            1
                        } else {
                            0
                        },
                    ),
                    "sltiu" => self.write_reg(
                        Register::X(rd),
                        if sign_extend(11, imm) > self.read_reg(Register::X(rs1)) {
                            1
                        } else {
                            0
                        },
                    ),
                    "xori" => self.write_reg(
                        Register::X(rd),
                        self.read_reg(Register::X(rs1)) ^ sign_extend(11, imm),
                    ),
                    "srli" => self.write_reg(
                        Register::X(rd),
                        self.read_reg(Register::X(rs1)) >> (imm & 0x3f),
                    ),
                    "srai" => self.write_reg(
                        Register::X(rd),
                        sign_extend(
                            63 - (imm as u8 & 0x3f),
                            self.read_reg(Register::X(rs1)) >> (imm & 0x3f),
                        ),
                    ),
                    "andi" => self.write_reg(
                        Register::X(rd),
                        self.read_reg(Register::X(rs1)) & sign_extend(11, imm),
                    ),
                    "ori" => self.write_reg(
                        Register::X(rd),
                        self.read_reg(Register::X(rs1)) | sign_extend(11, imm),
                    ),
                    "addiw" => self.write_reg(
                        Register::X(rd),
                        sign_extend(
                            31,
                            self.read_reg(Register::X(rs1))
                                .wrapping_add(sign_extend(11, imm))
                                & 0xffffffff,
                        ),
                    ),
                    "slliw" => self.write_reg(
                        Register::X(rd),
                        sign_extend(
                            31,
                            (self.read_reg(Register::X(rs1)) << (imm & 0x1f)) & 0xffffffff,
                        ),
                    ),
                    "srliw" => self.write_reg(
                        Register::X(rd),
                        sign_extend(
                            31,
                            (self.read_reg(Register::X(rs1)) & 0xffffffff) >> (imm & 0x1f)
                                & 0xffffffff,
                        ),
                    ),
                    "sraiw" => self.write_reg(
                        Register::X(rd),
                        sign_extend(
                            31 - (imm & 0x1f) as u8,
                            ((self.read_reg(Register::X(rs1)) & 0xffffffff) >> (imm & 0x1f))
                                & 0xffffffff,
                        ),
                    ),
                    "jalr" => {
                        let offset = sign_extend(11, imm);

                        let pc = self.read_reg(Register::Pc);
                        let dst = self.read_reg(Register::X(rs1)).wrapping_add(offset) & !1;

                        self.check_misaligned(dst)?;

                        self.write_reg(Register::X(rd), pc + 4);
                        self.write_reg(Register::Pc, dst);
                        self.inst.set_class(InstClass::Jump(true));
                    }
                    "csrrw" => {
                        let csr = if rd != 0 { self.read_csr(imm)? } else { 0 };
                        self.write_csr(imm, self.read_reg(Register::X(rs1)))?;

                        if rd != 0 {
                            self.write_reg(Register::X(rd), csr);
                        }
                    }
                    "csrrs" => {
                        let csr = self.read_csr(imm)?;
                        let rs1 = self.read_reg(Register::X(rs1));

                        self.write_reg(Register::X(rd), csr);

                        if rs1 != 0 {
                            self.write_csr(imm, csr | rs1)?;
                        }
                    }
                    "csrrc" => {
                        let csr = self.read_csr(imm)?;
                        let rs1 = self.read_reg(Register::X(rs1));

                        self.write_reg(Register::X(rd), csr);

                        if rs1 != 0 {
                            self.write_csr(imm, csr & !rs1)?;
                        }
                    }
                    "csrrwi" => {
                        let csr = if rd != 0 { self.read_csr(imm)? } else { 0 };

                        self.write_csr(imm, rs1 as u64)?;

                        if rd != 0 {
                            self.write_reg(Register::X(rd), csr);
                        }
                    }
                    "csrrsi" => {
                        let csr = self.read_csr(imm)?;

                        self.write_reg(Register::X(rd), csr);

                        if rs1 != 0 {
                            self.write_csr(imm, csr | rs1 as u64)?;
                        }
                    }
                    "csrrci" => {
                        let csr = self.read_csr(imm)?;

                        self.write_reg(Register::X(rd), csr);

                        if rs1 != 0 {
                            self.write_csr(imm, csr & !rs1 as u64)?;
                        }
                    }
                    _ => unimplemented!(),
                }
            }
            J => {
                let (rd, imm) = extract_j_type(self.inst.raw());
                let offset = sign_extend(20, imm);

                let pc = self.read_reg(Register::Pc);
                let dst = pc.wrapping_add(offset);

                // ターゲットアドレスが4byteでアライメントされていない場合はアライメントの例外を起こす。

                self.check_misaligned(dst)?;

                self.write_reg(Register::X(rd), pc + 4);
                self.write_reg(Register::Pc, dst);
                self.inst.set_class(InstClass::Jump(true));
            }
            R => {
                let (rd, rs1, rs2, _) = extract_r_type(self.inst.raw());

                match name {
                    "add" => self.write_reg(
                        Register::X(rd),
                        self.read_reg(Register::X(rs1))
                            .wrapping_add(self.read_reg(Register::X(rs2))),
                    ),
                    "mul" => self.write_reg(
                        Register::X(rd),
                        self.read_reg(Register::X(rs1))
                            .wrapping_mul(self.read_reg(Register::X(rs2))),
                    ),
                    "sub" => self.write_reg(
                        Register::X(rd),
                        self.read_reg(Register::X(rs1))
                            .wrapping_sub(self.read_reg(Register::X(rs2))),
                    ),
                    "sll" => self.write_reg(
                        Register::X(rd),
                        self.read_reg(Register::X(rs1)) << (self.read_reg(Register::X(rs2)) & 0x3f),
                    ),
                    "mulh" => {
                        let rs1 = sign_extend_128bit(63, self.read_reg(Register::X(rs1)) as u128);
                        let rs2 = sign_extend_128bit(63, self.read_reg(Register::X(rs2)) as u128);

                        self.write_reg(
                            Register::X(rd),
                            (((rs1 as i128) * (rs2 as i128)) >> 64) as u64,
                        );
                    }
                    "slt" => self.write_reg(
                        Register::X(rd),
                        if self.read_reg(Register::X(rs2)) as i64
                            > self.read_reg(Register::X(rs1)) as i64
                        {
                            1
                        } else {
                            0
                        },
                    ),
                    "mulhsu" => {
                        let rs1 = sign_extend_128bit(63, self.read_reg(Register::X(rs1)) as u128);
                        let rs2 = self.read_reg(Register::X(rs2)) as u128;

                        self.write_reg(Register::X(rd), (rs1.wrapping_mul(rs2) >> 64) as u64);
                    }
                    "sltu" => self.write_reg(
                        Register::X(rd),
                        if self.read_reg(Register::X(rs2)) > self.read_reg(Register::X(rs1)) {
                            1
                        } else {
                            0
                        },
                    ),
                    "mulhu" => {
                        let rs1 = self.read_reg(Register::X(rs1)) as u128;
                        let rs2 = self.read_reg(Register::X(rs2)) as u128;

                        self.write_reg(Register::X(rd), (rs1.wrapping_mul(rs2) >> 64) as u64);
                    }
                    "xor" => self.write_reg(
                        Register::X(rd),
                        self.read_reg(Register::X(rs1)) ^ self.read_reg(Register::X(rs2)),
                    ),
                    "div" => {
                        let rs1 = self.read_reg(Register::X(rs1));
                        let rs2 = self.read_reg(Register::X(rs2));

                        // 符号付きの割り算
                        // 符号のオーバーフローが起こった場合はrs1の値
                        // 0で割り算する場合はすべてのbitが１になっている値
                        // 以外は普通にわり算の値
                        // をrdにセットする。

                        self.write_reg(
                            Register::X(rd),
                            if rs1 == 1 << 63 && rs2 == !0 {
                                rs1
                            } else if rs2 == 0 {
                                u64::MAX
                            } else {
                                (rs1 as i64 / rs2 as i64) as u64
                            },
                        );
                    }
                    "sra" => {
                        let shift = self.read_reg(Register::X(rs2)) & 0x3f;

                        self.write_reg(
                            Register::X(rd),
                            sign_extend(63 - shift as u8, self.read_reg(Register::X(rs1)) >> shift),
                        );
                    }
                    "or" => self.write_reg(
                        Register::X(rd),
                        self.read_reg(Register::X(rs1)) | self.read_reg(Register::X(rs2)),
                    ),
                    "rem" => {
                        let rs1 = self.read_reg(Register::X(rs1));
                        let rs2 = self.read_reg(Register::X(rs2));

                        self.write_reg(
                            Register::X(rd),
                            if rs1 == 1 << 63 && rs2 == !0 {
                                0
                            } else if rs2 == 0 {
                                rs1
                            } else {
                                (rs1 as i64 % rs2 as i64) as u64
                            },
                        )
                    }
                    "srl" => {
                        let shift = self.read_reg(Register::X(rs2)) & 0x3f;

                        self.write_reg(Register::X(rd), self.read_reg(Register::X(rs1)) >> shift);
                    }
                    "divu" => {
                        let rs1 = self.read_reg(Register::X(rs1));
                        let rs2 = self.read_reg(Register::X(rs2));

                        self.write_reg(
                            Register::X(rd),
                            if rs2 == 0 { u64::MAX } else { rs1 / rs2 },
                        );
                    } // DIVU
                    "and" => self.write_reg(
                        Register::X(rd),
                        self.read_reg(Register::X(rs1)) & self.read_reg(Register::X(rs2)),
                    ),
                    "remu" => {
                        let rs1 = self.read_reg(Register::X(rs1));
                        let rs2 = self.read_reg(Register::X(rs2));

                        self.write_reg(Register::X(rd), if rs2 == 0 { rs1 } else { rs1 % rs2 });
                    }
                    "addw" => self.write_reg(
                        Register::X(rd),
                        sign_extend(
                            31,
                            self.read_reg(Register::X(rs1))
                                .wrapping_add(self.read_reg(Register::X(rs2)))
                                & 0xffffffff,
                        ),
                    ),
                    "mulw" => {
                        let rs1 = self.read_reg(Register::X(rs1)) & 0xffffffff;
                        let rs2 = self.read_reg(Register::X(rs2)) & 0xffffffff;

                        self.write_reg(Register::X(rd), sign_extend(31, (rs1 * rs2) & 0xffffffff));
                    }
                    "subw" => self.write_reg(
                        Register::X(rd),
                        sign_extend(
                            31,
                            self.read_reg(Register::X(rs1))
                                .wrapping_sub(self.read_reg(Register::X(rs2)))
                                & 0xffffffff,
                        ),
                    ),
                    "sllw" => self.write_reg(
                        Register::X(rd),
                        sign_extend(
                            31,
                            (self.read_reg(Register::X(rs1))
                                << (self.read_reg(Register::X(rs2)) & 0x1f))
                                & 0xffffffff,
                        ),
                    ),
                    "divw" => {
                        let rs1 = self.read_reg(Register::X(rs1)) as i32;
                        let rs2 = self.read_reg(Register::X(rs2)) as i32;

                        self.write_reg(
                            Register::X(rd),
                            if rs1 == i32::MIN && rs2 == !0 {
                                rs1 as u64
                            } else if rs2 == 0 {
                                u64::MAX
                            } else {
                                (rs1 / rs2) as u64
                            },
                        );
                    }
                    "srlw" => {
                        let shift = self.read_reg(Register::X(rs2)) & 0x1f;
                        self.write_reg(
                            Register::X(rd),
                            sign_extend(
                                31,
                                ((self.read_reg(Register::X(rs1)) & 0xffffffff) >> shift)
                                    & 0xffffffff,
                            ),
                        );
                    }
                    "divuw" => {
                        let rs1 = self.read_reg(Register::X(rs1));
                        let rs2 = self.read_reg(Register::X(rs2));

                        self.write_reg(
                            Register::X(rd),
                            if rs2 == 0 {
                                u64::MAX
                            } else {
                                sign_extend(31, (rs1 / rs2) & 0xffffffff)
                            },
                        );
                    }
                    "sraw" => {
                        let shift = self.read_reg(Register::X(rs2)) & 0x1f;
                        self.write_reg(
                            Register::X(rd),
                            sign_extend(
                                31 - shift as u8,
                                ((self.read_reg(Register::X(rs1)) & 0xffffffff) >> shift)
                                    & 0xffffffff,
                            ),
                        );
                    }
                    "remw" => {
                        let rs1 = self.read_reg(Register::X(rs1)) & 0xffffffff;
                        let rs2 = self.read_reg(Register::X(rs2)) & 0xffffffff;

                        self.write_reg(
                            Register::X(rd),
                            if rs1 == 1 << 31 && rs2 == 0xffffffff {
                                0
                            } else if rs2 == 0 {
                                sign_extend(31, rs1)
                            } else {
                                (rs1 as i32 % rs2 as i32) as u64
                            },
                        );
                    }
                    "remuw" => {
                        let rs1 = self.read_reg(Register::X(rs1)) & 0xffffffff;
                        let rs2 = self.read_reg(Register::X(rs2)) & 0xffffffff;

                        self.write_reg(
                            Register::X(rd),
                            sign_extend(31, if rs2 == 0 { rs1 } else { rs1 % rs2 }),
                        );
                    }
                    "sfence_vma" => {
                        if self.current_priv != Priv::S {
                            panic!("Error: executing sfence.vma in S mode is only supported.");
                        }

                        // よくわからないがriscv-testsでは
                        // mstatus.TVM == 0 && current_priv == S_MODE
                        // でも例外が発生するらしい。
                        // ここはissueを投げるか検討

                        return Err(IllegralInstruction);

                        //let tvm =
                        //    self.read_raw_csr(CSR_MSTATUS).unwrap() & CSR_MSTATUS_TVM_MASK;

                        //if tvm != 0 {
                        //    // tvmが設定されている場合はSモードで実行している場合は例外が発生する可能性があるらしい。
                        //    panic!("Error: sfence.vma is not supported when mstatus.TVM equals 1.");
                        //}
                    }
                    name if *self.inst.isa() == InstIsa::A => {
                        let addr = self.read_reg(Register::X(rs1)) as usize;

                        match name {
                            "amoswap_w" | "lr_w" | "sc_w" | "amoadd_w" | "amoand_w"
                            | "amoxor_w" | "amoor_w" | "amomin_w" | "amomax_w" | "amominu_w"
                            | "amomaxu_w" => {
                                // 32bit版の場合は4バイトアライメント
                                self.check_misaligned(addr as u64)?;

                                if name == "sc_w" {
                                    // SC.W

                                    if let Some(range) = self.pop_reserved_memory_range() {
                                        // 予約領域が存在している場合

                                        if range.0 <= addr && range.1 >= addr + 4 {
                                            // 予約領域内の場合はそのメモリ領域に書き込みを行い、rdに0を書き込む。
                                            self.write_memory(
                                                addr,
                                                &(self.read_reg(Register::X(rs2)) as u32)
                                                    .to_le_bytes(),
                                            )?;

                                            self.write_reg(Register::X(rd), 0);
                                        } else {
                                            // 上の条件に当てはまらない場合はrdに1を書き込むことにする。
                                            self.write_reg(Register::X(rd), 1);
                                        }
                                    } else {
                                        // ここで二回同じコードを書いているがif-let chainが使えるようになったら一つで済むようになる。
                                        self.write_reg(Register::X(rd), 1);
                                    }
                                } else {
                                    let v = u32::from_le_bytes(self.read_memory::<4>(addr)?);

                                    match name {
                                        "amoswap_w" => {
                                            self.write_memory(
                                                addr,
                                                &(self.read_reg(Register::X(rs2)) as u32)
                                                    .to_le_bytes(),
                                            )?;
                                            self.write_reg(Register::X(rs2), v as u64);
                                        }
                                        "lr_w" => {
                                            self.write_reg(
                                                Register::X(rd),
                                                sign_extend(31, v as u64),
                                            );
                                            self.push_reserved_memory_range((addr, addr + 4));
                                        }
                                        "amoadd_w" => self.write_memory(
                                            addr,
                                            &(v.wrapping_add(
                                                self.read_reg(Register::X(rs2)) as u32
                                            ))
                                            .to_le_bytes(),
                                        )?,
                                        "amoxor_w" => self.write_memory(
                                            addr,
                                            &(v ^ (self.read_reg(Register::X(rs2)) as u32))
                                                .to_le_bytes(),
                                        )?,
                                        "amoand_w" => self.write_memory(
                                            addr,
                                            &(v & (self.read_reg(Register::X(rs2)) as u32))
                                                .to_le_bytes(),
                                        )?,
                                        "amoor_w" => self.write_memory(
                                            addr,
                                            &(v | (self.read_reg(Register::X(rs2)) as u32))
                                                .to_le_bytes(),
                                        )?,

                                        "amomin_w" => {
                                            let rs2_val = self.read_reg(Register::X(rs2)) as u32;

                                            self.write_memory(
                                                addr,
                                                &(if rs2_val as i32 > v as i32 {
                                                    v
                                                } else {
                                                    rs2_val
                                                })
                                                .to_le_bytes(),
                                            )?;
                                        }
                                        "amomax_w" => {
                                            let rs2_val = self.read_reg(Register::X(rs2)) as u32;

                                            self.write_memory(
                                                addr,
                                                &(if v as i32 > rs2_val as i32 {
                                                    v
                                                } else {
                                                    rs2_val
                                                })
                                                .to_le_bytes(),
                                            )?;
                                        }
                                        "amominu_w" => self.write_memory(
                                            addr,
                                            &v.min(self.read_reg(Register::X(rs2)) as u32)
                                                .to_le_bytes(),
                                        )?,
                                        "amomaxu_w" => self.write_memory(
                                            addr,
                                            &v.max(self.read_reg(Register::X(rs2)) as u32)
                                                .to_le_bytes(),
                                        )?,
                                        _ => unimplemented!(),
                                    }

                                    self.write_reg(Register::X(rd), sign_extend(31, v as u64));
                                }
                            }
                            "amoswap_d" | "amoxor_d" | "amoadd_d" | "amoand_d" | "amoor_d"
                            | "amomin_d" | "amomax_d" | "amominu_d" | "amomaxu_d" => {
                                // 64bit版の場合は8バイトアライメント
                                self.check_misaligned_nbyte_misaligned(addr as u64, 8)?;

                                let v = u64::from_le_bytes(self.read_memory::<8>(addr)?);

                                match name {
                                    "amoswap_d" => {
                                        self.write_memory(
                                            addr,
                                            &self.read_reg(Register::X(rs2)).to_le_bytes(),
                                        )?;
                                        self.write_reg(Register::X(rs2), v);
                                    }
                                    "amoxor_d" => self.write_memory(
                                        addr,
                                        &(v ^ self.read_reg(Register::X(rs2))).to_le_bytes(),
                                    )?,
                                    "amoadd_d" => self.write_memory(
                                        addr,
                                        &(v.wrapping_add(self.read_reg(Register::X(rs2))))
                                            .to_le_bytes(),
                                    )?,
                                    "amoand_d" => self.write_memory(
                                        addr,
                                        &(v & self.read_reg(Register::X(rs2))).to_le_bytes(),
                                    )?,
                                    "amoor_d" => self.write_memory(
                                        addr,
                                        &(v | self.read_reg(Register::X(rs2))).to_le_bytes(),
                                    )?,
                                    "amomin_d" => {
                                        let rs2_val = self.read_reg(Register::X(rs2));

                                        self.write_memory(
                                            addr,
                                            &(if rs2_val as i64 > v as i64 {
                                                v
                                            } else {
                                                rs2_val
                                            })
                                            .to_le_bytes(),
                                        )?;
                                    }
                                    "amomax_d" => {
                                        let rs2_val = self.read_reg(Register::X(rs2));

                                        self.write_memory(
                                            addr,
                                            &(if v as i64 > rs2_val as i64 {
                                                v
                                            } else {
                                                rs2_val
                                            })
                                            .to_le_bytes(),
                                        )?;
                                    }
                                    "amominu_d" => self.write_memory(
                                        addr,
                                        &v.min(self.read_reg(Register::X(rs2))).to_le_bytes(),
                                    )?,
                                    "amomaxu_d" => self.write_memory(
                                        addr,
                                        &v.max(self.read_reg(Register::X(rs2))).to_le_bytes(),
                                    )?,
                                    _ => unimplemented!(),
                                }

                                self.write_reg(Register::X(rd), v);
                            }
                            _ => unimplemented!(),
                        }
                    }
                    _ => unimplemented!(),
                }
            }
            S => {
                let (rs1, rs2, imm) = extract_s_type(self.inst.raw());

                match name {
                    "sb" => {
                        let bytes = (self.read_reg(Register::X(rs2)) as u8).to_le_bytes();

                        self.write_memory(
                            self.read_reg(Register::X(rs1))
                                .wrapping_add(sign_extend(11, imm))
                                as usize,
                            &bytes,
                        )?;
                    }
                    "sh" => {
                        let bytes = (self.read_reg(Register::X(rs2)) as u16).to_le_bytes();

                        self.write_memory(
                            self.read_reg(Register::X(rs1))
                                .wrapping_add(sign_extend(11, imm))
                                as usize,
                            &bytes,
                        )?;
                    }
                    "sw" => {
                        let bytes = (self.read_reg(Register::X(rs2)) as u32).to_le_bytes();

                        self.write_memory(
                            self.read_reg(Register::X(rs1))
                                .wrapping_add(sign_extend(11, imm))
                                as usize,
                            &bytes,
                        )?;
                    }
                    "sd" => {
                        let bytes = self.read_reg(Register::X(rs2)).to_le_bytes();

                        self.write_memory(
                            self.read_reg(Register::X(rs1))
                                .wrapping_add(sign_extend(11, imm))
                                as usize,
                            &bytes,
                        )?;
                    }
                    _ => unimplemented!(),
                }
            }
            U => {
                let (rd, imm) = extract_u_type(self.inst.raw());

                match name {
                    "auipc" => self.write_reg(
                        Register::X(rd),
                        self.read_reg(Register::Pc)
                            .wrapping_add(sign_extend(31, imm)),
                    ),
                    "lui" => self.write_reg(Register::X(rd), sign_extend(31, imm)),
                    _ => return Err(IllegralInstruction),
                }
            }
            Ca => {
                let (rd, imm) = extract_ci_type(self.inst.raw() as u16);
                let rd = convert_from_c_reg_to_i(rd as u16 & 0x7);
                let rs2 = convert_from_c_reg_to_i(imm as u16 & 0x7);

                match name {
                    "c_sub" => self.write_reg(
                        Register::X(rd),
                        self.read_reg(Register::X(rd))
                            .wrapping_sub(self.read_reg(Register::X(rs2))),
                    ),
                    "c_xor" => self.write_reg(
                        Register::X(rd),
                        self.read_reg(Register::X(rd)) ^ self.read_reg(Register::X(rs2)),
                    ),
                    "c_or" => self.write_reg(
                        Register::X(rd),
                        self.read_reg(Register::X(rd)) | self.read_reg(Register::X(rs2)),
                    ),
                    "c_and" => self.write_reg(
                        Register::X(rd),
                        self.read_reg(Register::X(rd)) & self.read_reg(Register::X(rs2)),
                    ),
                    "c_subw" => self.write_reg(
                        Register::X(rd),
                        sign_extend(
                            31,
                            self.read_reg(Register::X(rd))
                                .wrapping_sub(self.read_reg(Register::X(rs2)))
                                & 0xffffffff,
                        ),
                    ),
                    "c_addw" => self.write_reg(
                        Register::X(rd),
                        sign_extend(
                            31,
                            self.read_reg(Register::X(rd))
                                .wrapping_add(self.read_reg(Register::X(rs2)))
                                & 0xffffffff,
                        ),
                    ),
                    _ => unimplemented!(),
                }
            }
            Cb => {
                let (rd, offset) = extract_cb_type(self.inst.raw() as u16);
                let imm = ((offset >> 2) & 0x20) | (offset & 0x1f);

                match name {
                    "c_srli" => {
                        if imm != 0 {
                            self.write_reg(Register::X(rd), self.read_reg(Register::X(rd)) >> imm);
                        } else {
                            // imm=0の場合はHINTsをエンコードするらしい。
                        }
                    }
                    "c_srai" => {
                        if imm != 0 {
                            self.write_reg(
                                Register::X(rd),
                                sign_extend(63 - imm as u8, self.read_reg(Register::X(rd)) >> imm),
                            );
                        } else {
                            // imm=0の場合はHINTsをエンコードするらしい。
                        }
                    }
                    "c_andi" => self.write_reg(
                        Register::X(rd),
                        self.read_reg(Register::X(rd)) & sign_extend(5, imm),
                    ),
                    "c_beqz" | "c_bnez" => {
                        let offset = ((offset << 1) & 0x100)
                            | ((offset << 3) & 0xc0)
                            | ((offset << 5) & 0x20)
                            | ((offset >> 2) & 0x18)
                            | (offset & 0x6);

                        let rs1 = self.read_reg(Register::X(rd));

                        // C.BEQZ or C.BNEZ
                        if (name == "c_beqz" && rs1 == 0) || (name == "c_bnez" && rs1 != 0) {
                            self.write_reg(
                                Register::Pc,
                                self.read_reg(Register::Pc)
                                    .wrapping_add(sign_extend(8, offset)),
                            );

                            self.inst.set_class(InstClass::Jump(true));
                        }
                    }
                    _ => unimplemented!(),
                }
            }
            Cj => match name {
                "c_j" => {
                    let imm = (self.inst.raw() >> 1) & 0xffe;
                    let offset = (imm & 0xb40)
                        | ((imm << 3) & 0x400)
                        | ((imm << 2) & 0x80)
                        | ((imm << 4) & 0x20)
                        | ((imm >> 6) & 0x10)
                        | ((imm >> 1) & 0xe);

                    self.write_reg(
                        Register::Pc,
                        self.read_reg(Register::Pc)
                            .wrapping_add(sign_extend(11, offset as u64)),
                    );

                    self.inst.set_class(InstClass::Jump(true));
                }
                _ => unimplemented!(),
            },
            Ci => {
                let (rd, imm) = extract_ci_type(self.inst.raw() as u16);

                match name {
                    "c_nop" => {}
                    "c_addi" => {
                        if imm == 0 {
                            panic!("Error: Ths imm of C.ADDI is not zero.");
                        }

                        self.write_reg(
                            Register::X(rd),
                            self.read_reg(Register::X(rd))
                                .wrapping_add(sign_extend(5, imm)),
                        );
                    }
                    "c_addiw" => {
                        if rd != 0 {
                            self.write_reg(
                                Register::X(rd),
                                sign_extend(
                                    31,
                                    self.read_reg(Register::X(rd))
                                        .wrapping_add(sign_extend(5, imm))
                                        & 0xffffffff,
                                ),
                            );
                        } else {
                            // rd=0は予約済み
                            panic!("Error: x0 is not zero with c_addiw.");
                        }
                    }
                    "c_li" => {
                        if rd != 0 {
                            self.write_reg(Register::X(rd), sign_extend(5, imm));
                        } else {
                            // rd=0の場合はHINTsをエンコードするらしい。
                        }
                    }
                    "c_lui" => {
                        if imm == 0 {
                            panic!("Error: x0 is not zero with c_lui.");
                        }

                        let nzimm = imm << 12;

                        self.write_reg(Register::X(rd), sign_extend(17, nzimm));
                    }
                    "c_addi16sp" => {
                        if rd == 0 {
                            // rd=0は予約済み
                            panic!("Error: x0 is not zero with c_addi16sp.");
                        } else {
                            let nzimm = ((imm << 4) & 0x200)
                                | ((imm << 6) & 0x180)
                                | ((imm << 3) & 0x40)
                                | ((imm << 5) & 0x20)
                                | (imm & 0x10);

                            self.write_reg(
                                Register::X(2),
                                self.read_reg(Register::X(2))
                                    .wrapping_add(sign_extend(9, nzimm)),
                            );
                        }
                    }
                    "c_slli" => {
                        if rd != 0 && imm != 0 {
                            self.write_reg(Register::X(rd), self.read_reg(Register::X(rd)) << imm);
                        } else {
                            // rd=0またはimm=0の場合はHINTsをエンコードするらしい。
                        }
                    }
                    "c_lwsp" => {
                        if rd == 0 {
                            panic!("Error: Ths rd of {} is not zero.", name);
                        }

                        let offset = ((imm << 6) & 0xc0) | (imm & 0x3c);

                        let bytes = self.read_memory::<4>(
                            self.read_reg(Register::X(2)).wrapping_add(offset) as usize,
                        )?;

                        self.write_reg(
                            Register::X(rd),
                            sign_extend(31, u32::from_le_bytes(bytes) as u64),
                        );
                    }
                    "c_ldsp" => {
                        if rd == 0 {
                            panic!("Error: Ths rd of {} is not zero.", name);
                        }

                        let offset = ((imm << 6) & 0xc0) | (imm & 0x3c);

                        self.write_reg(
                            Register::X(rd),
                            u64::from_le_bytes(self.read_memory::<8>(
                                self.read_reg(Register::X(2)).wrapping_add(offset) as usize,
                            )?),
                        );
                    }
                    _ => unimplemented!(),
                }
            }
            Ciw => {
                let (rd, imm) = extract_ciw_type(self.inst.raw() as u16);

                match name {
                    "c_addi4spn" => {
                        if imm == 0 {
                            // 予約されている。
                            return Err(IllegralInstruction);
                        }

                        let nzuimm = ((imm & 0x3c) << 4)
                            | ((imm & 0xc0) >> 2)
                            | ((imm & 0x1) << 3)
                            | ((imm & 0x2) << 1);
                        self.write_reg(
                            Register::X(rd),
                            self.read_reg(Register::X(2)).wrapping_add(nzuimm),
                        );
                    }
                    _ => unimplemented!(),
                }
            }
            Cr => {
                let (rs2, rd) = extract_cr_type(self.inst.raw() as u16);

                match name {
                    "c_jr" | "c_jalr" => {
                        if rd == 0 {
                            panic!("Error: Ths rd of {} is not zero.", name);
                        }

                        if name == "c_jalr" {
                            self.write_reg(
                                Register::X(1),
                                self.read_reg(Register::Pc).wrapping_add(2),
                            );
                        }

                        self.write_reg(Register::Pc, self.read_reg(Register::X(rd)) & !1);
                        self.inst.set_class(InstClass::Jump(true));
                    }
                    "c_mv" => {
                        if rd == 0 {
                            // rd=0の場合はHINTsをエンコードするらしい。
                        } else {
                            self.write_reg(Register::X(rd), self.read_reg(Register::X(rs2)));
                        }
                    }
                    "c_add" => {
                        if rd == 0 {
                            // rd=0の場合はHINTsをエンコードするらしい。
                        } else {
                            self.write_reg(
                                Register::X(rd),
                                self.read_reg(Register::X(rd))
                                    .wrapping_add(self.read_reg(Register::X(rs2))),
                            );
                        }
                    }
                    _ => unimplemented!(),
                }
            }
            Cl | Cs => {
                // CL: (rd, rs1, imm)
                // CS: (rs2, rs1, imm)
                let (fr, sr, imm) = extract_clcs_type(self.inst.raw() as u16);

                match name {
                    "c_lw" => {
                        let offset = calc_c_offset_5_3_2_6(imm);

                        let bytes = self.read_memory(
                            self.read_reg(Register::X(sr)).wrapping_add(offset) as usize,
                        )?;

                        self.write_reg(
                            Register::X(fr),
                            sign_extend(31, u32::from_le_bytes(bytes) as u64),
                        );
                    }
                    "c_ld" => {
                        let offset = calc_c_offset_5_3_7_6(imm);
                        let bytes = self.read_memory::<8>(
                            self.read_reg(Register::X(sr)).wrapping_add(offset) as usize,
                        )?;

                        self.write_reg(Register::X(fr), u64::from_le_bytes(bytes));
                    }
                    "c_sw" => {
                        let offset = calc_c_offset_5_3_2_6(imm);
                        let bytes = (self.read_reg(Register::X(fr)) as u32).to_le_bytes();

                        self.write_memory(
                            self.read_reg(Register::X(sr)).wrapping_add(offset) as usize,
                            &bytes,
                        )?;
                    }
                    "c_sd" => {
                        let offset = calc_c_offset_5_3_7_6(imm);
                        let bytes = self.read_reg(Register::X(fr)).to_le_bytes();

                        self.write_memory(
                            self.read_reg(Register::X(sr)).wrapping_add(offset) as usize,
                            &bytes,
                        )?;
                    }
                    _ => unimplemented!(),
                }
            }
            Css => {
                let (rs2, imm) = extract_css_type(self.inst.raw() as u16);

                match name {
                    "c_swsp" => {
                        let offset = ((imm << 6) & 0xc0) | (imm & 0x3c);

                        let bytes = (self.read_reg(Register::X(rs2)) as u32).to_le_bytes();

                        self.write_memory(
                            self.read_reg(Register::X(2)).wrapping_add(offset) as usize,
                            &bytes,
                        )?;
                    }
                    "c_sdsp" => {
                        let offset = ((imm << 6) & 0x7) | (imm & 0x38);

                        self.write_memory(
                            self.read_reg(Register::X(2)).wrapping_add(offset) as usize,
                            &self.read_reg(Register::X(rs2)).to_le_bytes(),
                        )?;
                    }
                    _ => unimplemented!(),
                }
            }
            Other => match name {
                "fence" => {
                    // 並行処理系の工夫する構造はないので作るまでは実装しない。
                    eprintln!("[warning]: fence may not work properly.");

                    match self.inst.raw() {
                        0x8330000f | 0x0100000f => {
                            // FENCE.TSO PAUSEは実装していない
                            return Err(IllegralInstruction);
                        } // FENCE.TSO PAUSE
                        _ => {} //fence
                    }
                }
                "ecall" => match self.current_priv {
                    Priv::M => return Err(EnvironmentCallFromMMode),
                    Priv::S => return Err(EnvironmentCallFromSMode),
                    Priv::U => return Err(EnvironmentCallFromUMode),
                },
                "sret" => {
                    use Priv::*;

                    match self.current_priv {
                        M | S => {
                            let mstatus = self.read_raw_csr(CSR_MSTATUS).unwrap();

                            if self.current_priv == S && mstatus & CSR_MSTATUS_TSR_MASK != 0 {
                                // Sモードでmstatus.TSRが有効な場合はIllegralInstructionを起こす。
                                return Err(IllegralInstruction);
                            }

                            let spp = (mstatus & CSR_MSTATUS_SPP_MASK) >> 8;
                            let spie = (mstatus & CSR_MSTATUS_SPIE_MASK) >> 5;
                            let new_sstaus = (mstatus
                                & !CSR_SSTATUS_MASK
                                & !CSR_MSTATUS_SPP_MASK
                                & !CSR_MSTATUS_SPIE_MASK
                                & !CSR_MSTATUS_SIE_MASK)
                                | ((Priv::U as u64) << 8)
                                | (1 << 8)
                                | (spie << 1);

                            self.write_csr(CSR_SSTATUS, new_sstaus).unwrap();
                            self.write_reg(Register::Pc, self.read_csr(CSR_SEPC).unwrap());
                            self.current_priv = Priv::from(spp);

                            eprintln!("current_priv: {:?}", self.current_priv);
                            self.inst.set_class(InstClass::Jump(true));
                        }
                        _ => return Err(IllegralInstruction),
                    }
                }
                "mret" => {
                    match self.current_priv {
                        Priv::M => {
                            let mstatus = self.read_raw_csr(CSR_MSTATUS).unwrap();

                            let mpp = (mstatus & CSR_MSTATUS_MPP_MASK) >> 11;
                            let mpie = (mstatus & CSR_MSTATUS_MPIE_MASK) >> 7;
                            // Memory Privilege in mstatus Registerは後実装する。
                            // let mprv = if mpp == Priv::M as u64 { 1 } else { 0 };

                            let new_mstatus = (mstatus
                                & !CSR_MSTATUS_MIE_MASK
                                & !CSR_MSTATUS_MPP_MASK
                                & !(CSR_MSTATUS_MPIE_MASK))
                                | (mpie << 3)
                                | (1 << 7)
                                | ((Priv::U as u64) << 11);

                            self.write_csr(CSR_MSTATUS, new_mstatus).unwrap();
                            self.write_reg(Register::Pc, self.read_csr(CSR_MEPC).unwrap());
                            self.current_priv = Priv::from(mpp);

                            eprintln!("current_priv: {:?}", self.current_priv);
                            self.inst.set_class(InstClass::Jump(true));
                        } // MRET
                        _ => {
                            // Mモード以外で呼び出された場合は実装していない。
                            return Err(IllegralInstruction);
                        }
                    }
                }
                "wfi" => {
                    if self.current_priv != Priv::S {
                        panic!("Error: wfi in only S MODE is supported.");
                    }

                    let tw = self.read_raw_csr(CSR_MSTATUS).unwrap() & CSR_MSTATUS_TW_MASK;

                    if tw == 0 {
                        // timeoutがないとき
                        eprintln!("[info]: Starting wfi loop...");
                        loop {
                            // mi{e,p}の値がそれぞれセットされている場合はxstatus.MIEにかかわらず終了する。
                            let active = self.read_raw_csr(CSR_MIE).unwrap()
                                & self.read_raw_csr(CSR_MIP).unwrap();

                            if active != 0 {
                                if active.count_ones() != 1 {
                                    panic!("Error: Nested traps are not supported.");
                                }

                                match active {
                                    2 => break,
                                    _ => panic!("Error: The active interrupt is not suported."),
                                }
                            }
                        }
                        eprintln!("[info]: Ending wfi loop...");
                    } else {
                        // timeoutがあるとき
                        panic!("Error: tw of wfi is not supported.");
                    }
                }
                _ => unimplemented!(),
            },
        };

        Ok(())
    }

    // 実行した命令に応じてPCを進める関数
    fn progress_pc(&mut self) {
        if *self.inst.isa() == InstIsa::C {
            self.pc += 2;
        } else {
            self.pc += 4;
        }
    }

    fn handle_exception(&mut self, e: Exception) {
        use crate::exception::Exception::*;

        eprintln!("EXCEPTION: {:?}", e);

        let is_interrupt = e as u64 >> 63 == 1;
        let medeleg = self.read_raw_csr(CSR_MEDELEG).unwrap();
        let mideleg = self.read_raw_csr(CSR_MIDELEG).unwrap();
        let cause = if is_interrupt {
            e as u64 & !(1 << 63)
        } else {
            e as u64
        };

        if self.current_priv != Priv::M
            && ((!is_interrupt && (medeleg >> cause) & 0x1 != 0)
                || (is_interrupt && (mideleg >> cause) & 0x1 != 0))
        {
            // 委譲
            let spp = self.current_priv as u64;
            self.current_priv = Priv::S;

            let sstatus = self.read_raw_csr(CSR_SSTATUS).unwrap();
            let spie = (sstatus & CSR_MSTATUS_SIE_MASK) >> 1;

            let sepc = if is_interrupt { self.pc + 4 } else { self.pc };
            self.write_raw_csr(CSR_SEPC, sepc).unwrap();

            let next_sstatus =
                sstatus & !CSR_MSTATUS_SPP_MASK & !CSR_MSTATUS_SPIE_MASK & !CSR_MSTATUS_SIE_MASK
                    | (spp << 8)
                    | (spie << 5);
            self.write_raw_csr(CSR_SSTATUS, next_sstatus).unwrap();

            self.write_raw_csr(CSR_SCAUSE, e as u64).unwrap();
        } else {
            let mpp = self.current_priv as u64;
            self.current_priv = Priv::M;

            let mstatus = self.read_raw_csr(CSR_MSTATUS).unwrap();
            let mpie = (mstatus & CSR_MSTATUS_MIE_MASK) >> 3;

            if is_interrupt {
                // 割り込み
                // 次の命令のアドレスを保存する。wfiのところに記述がある。
                self.write_raw_csr(CSR_MEPC, self.pc + 4).unwrap();
            } else {
                // 例外
                self.write_raw_csr(CSR_MEPC, self.pc).unwrap();
            }

            let next_mstatus =
                (mstatus & !CSR_MSTATUS_MPP_MASK & !CSR_MSTATUS_MPIE_MASK & !CSR_MSTATUS_MIE_MASK)
                    | (mpp << 11)
                    | (mpie << 7);
            self.write_raw_csr(CSR_MSTATUS, next_mstatus).unwrap();

            self.write_raw_csr(CSR_MCAUSE, e as u64).unwrap();
        }

        let xtvec = if self.current_priv == Priv::M {
            self.read_raw_csr(CSR_MTVEC).unwrap()
        } else {
            self.read_raw_csr(CSR_STVEC).unwrap()
        };

        match e {
            EnvironmentCallFromMMode
            | EnvironmentCallFromUMode
            | EnvironmentCallFromSMode
            | InstructionAddressMissaligned => {
                // 同期例外の場合はモードにかかわらずpcにBASEを設定する。
                // 多分ハンドラがmcauseの値からどの処理を行うかを判定する感じかな。
                self.exception_direct_jump(xtvec);
            }
            IllegralInstruction => {
                let inst = self.inst.raw();

                // 命令が0、C拡張が有効でなく、C命令の場合はとりあえず不正命令の処理を行う
                // C拡張が有効でなく、実行した命令がC拡張の命令の場合も不正命令の処理を行う。
                // これは実装していない命令を見つけるための処置である。
                if (*self.inst.isa() != InstIsa::C && self.inst.raw() == 0)
                    || (!self.is_c_extension_enabled() && *self.inst.isa() == InstIsa::C)
                {
                    let xtval = inst;

                    if self.current_priv == Priv::M {
                        self.write_raw_csr(CSR_MTVAL, xtval as u64).unwrap();
                    } else {
                        self.write_raw_csr(CSR_STVAL, xtval as u64).unwrap();
                    }

                    self.exception_direct_jump(xtvec);
                } else {
                    let op = self.inst.op();
                    let funct3 = (inst >> 12) & 0x7;
                    let funct7 = inst >> 25;

                    if op == 0b1110011
                        && ((funct3 == 0b001 || funct3 == 0b010 || funct3 == 0b011)
                            || (funct3 == 0 && funct7 == 0b1001)
                            || inst == 0x10200073)
                    {
                        // CSRRW
                        // CSRRS
                        // SFENCE.VMA
                        // SRET
                        // この実装だと実装していないCSRを読み込むときはriscv-testsが失敗する想定
                        // 正しく例外を起こしている（実装済みで正常な例外）場合はmtvalを設定し、同期例外の処理を行う。

                        if self.current_priv == Priv::M {
                            self.write_raw_csr(CSR_MTVAL, inst as u64).unwrap();
                        } else {
                            self.write_raw_csr(CSR_STVAL, inst as u64).unwrap();
                        }

                        self.exception_direct_jump(xtvec);
                    } else {
                        // 実装していない可能性がある命令はこっち
                        panic!(
                            "instruction: 0x{:08x} op: 0b{:07b} funct3: 0b{:03b}\nException: {:?}",
                            inst, op, funct3, e
                        );
                    }
                }
            }
            SuperSoftInt => {
                self.interupt_vectored_jump(xtvec, e as u64);
            }
        }
    }

    pub fn run(&mut self) {
        loop {
            if self.riscv_tests_finished {
                break;
            }

            eprintln!("PC: 0x{:016x}", self.pc,);
            let raw_inst = self.fetch();

            self.inst = self.decode(raw_inst);

            match self.exec() {
                Err(e) => self.handle_exception(e),
                Ok(_) => {
                    self.add_cycle();

                    if let Err(e) = self.check_interrupt_active() {
                        self.handle_exception(e);
                        continue;
                    }

                    if InstClass::Jump(true) != *self.inst.class() {
                        self.progress_pc();
                    }
                }
            }
        }
    }

    // C拡張が有効かどうかを確認する関数
    pub fn is_c_extension_enabled(&self) -> bool {
        (self.read_raw_csr(CSR_MISA).unwrap() & 0x4) != 0
    }

    fn exception_direct_jump(&mut self, xtvec: u64) {
        let base = xtvec & !0x3;

        self.write_reg(Register::Pc, base);
    }

    fn interupt_vectored_jump(&mut self, xtvec: u64, xcause: u64) {
        if self.current_priv != Priv::M {
            panic!("Error: The vectored jump in only M mode is supported.");
        }

        let base = xtvec & !0x3;
        let cause = xcause & !(1 << 63);

        self.write_reg(Register::Pc, base + cause * 4);
    }

    pub fn show_regs(&self) {
        eprintln!("---------- REGS ----------");
        eprintln!("x00: 0x{:016x}", 0);

        for (i, reg) in self.regs.iter().enumerate() {
            eprintln!("x{:02}: 0x{:016x}", i + 1, reg);
        }
        eprintln!("---------- REGS ----------");
    }

    // riscv-testsが成功しているかどうかを確認する関数
    pub fn check_riscv_tests_result(&self) -> bool {
        self.read_memory::<4>(self.riscv_tests_exit_memory_address)
            .unwrap()
            == [1, 0, 0, 0]
    }

    // riscv-testsが終了するメモリアドレスを指定する関数
    pub fn set_riscv_tests_exit_memory_address(&mut self, address: usize) {
        self.riscv_tests_exit_memory_address = address;
    }
}
