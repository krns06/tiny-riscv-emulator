use std::{error::Error, path::Path};

use crate::{
    csr::{
        Csr, CSR_MCAUSE, CSR_MEDELEG, CSR_MEPC, CSR_MIDELEG, CSR_MISA, CSR_MSTATUS,
        CSR_MSTATUS_MIE_MASK, CSR_MSTATUS_MPIE_MASK, CSR_MSTATUS_MPP_MASK, CSR_MTVAL, CSR_MTVEC,
    },
    exception::Exception::*,
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

fn calc_c_offset_5_3_2_6(imm: u64) -> u64 {
    ((imm << 6) & 0x40) | ((imm << 1) & 0x38) | ((imm << 1) & 0x4)
}

fn calc_c_offset_5_3_7_6(imm: u64) -> u64 {
    ((imm << 6) & 0xc0) | ((imm << 1) & 0x38)
}

// エミュレータがexecしたときにその命令が何であるかを伝える列挙体
// jump系の命令だと命令後にpc+4をしなくて良くなるのでそれを伝えたりする。←これ以外の用途があるかはわからない。
enum EmulatorFlag {
    Jump,
    Common,
    ExeC, // C拡張の命令を実行した場合
}

#[derive(Default)]
pub struct Emulator {
    pub(crate) memory: Memory<MEMORY_SIZE>,
    pub(crate) regs: [u64; 31],
    pub(crate) csr: Csr,
    pub(crate) pc: u64,
    pub(crate) current_priv: Priv,
    pub(crate) instruction: u32,
    pub(crate) reserved_memory_ranges: Vec<(usize, usize)>, // 予約されたメモリ領域を指定する。(begin, end)
    pub(crate) c_instruction: u16,                          // C拡張の命令を格納

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
    fn read_reg(&self, reg: Register) -> Result<u64> {
        use crate::register::Register::*;

        match reg {
            X(0) => Ok(0),
            X(i) => {
                if i > 31 {
                    panic!("Error: Unknown register x{}.", i);
                } else {
                    Ok(self.regs[i as usize - 1])
                }
            }
            Pc => Ok(self.pc),
        }
    }

    // レジスタを書き込むときに使用する関数
    fn write_reg(&mut self, reg: Register, value: u64) -> Result<()> {
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

        Ok(())
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
    // 暗黙的なメモリ読み込みはいくつか定義されているものを除き、例外は発生しない。
    // 現在読んでいる仕様書の部分ではfetchに相当する部分について特に明記されていないので現在のところは例外が起こらない想定とする。
    // run以外から呼んではいけない。
    fn fetch(&mut self) {
        self.instruction = u32::from_le_bytes(self.memory.read::<4>(self.pc as usize));
    }

    // C拡張の形式の命令を実行する関数
    fn c_exec(&mut self) -> Result<EmulatorFlag> {
        self.c_instruction = self.instruction as u16;

        // op
        match self.c_instruction & 0x3 {
            0b00 => {
                // CL: (rd, rs1, imm)
                // CS: (rs2, rs1, imm)
                let (fr, sr, imm) = extract_clcs_type(self.c_instruction);

                // funct3
                match self.c_instruction >> 13 {
                    0 => {
                        let (rd, imm) = extract_ciw_type(self.c_instruction);

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
                            self.read_reg(Register::X(2))?.wrapping_add(nzuimm),
                        )?;
                    } // C.ADDI4SPN
                    0b010 => {
                        let offset = calc_c_offset_5_3_2_6(imm);

                        let bytes = self.read_memory(
                            self.read_reg(Register::X(sr))?.wrapping_add(offset) as usize,
                        )?;

                        self.write_reg(
                            Register::X(fr),
                            sign_extend(31, u32::from_le_bytes(bytes) as u64),
                        )?;
                    } //C.LW
                    0b011 => {
                        let offset = calc_c_offset_5_3_7_6(imm);
                        let bytes = self.read_memory::<8>(
                            self.read_reg(Register::X(sr))?.wrapping_add(offset) as usize,
                        )?;

                        self.write_reg(Register::X(fr), u64::from_le_bytes(bytes))?;
                    } // C.LD
                    0b110 => {
                        let offset = calc_c_offset_5_3_2_6(imm);
                        let bytes = (self.read_reg(Register::X(fr))? as u32).to_le_bytes();

                        self.write_memory(
                            self.read_reg(Register::X(sr))?.wrapping_add(offset) as usize,
                            &bytes,
                        )?;
                    } // C.SW
                    0b111 => {
                        let offset = calc_c_offset_5_3_7_6(imm);
                        let bytes = self.read_reg(Register::X(fr))?.to_le_bytes();

                        self.write_memory(
                            self.read_reg(Register::X(sr))?.wrapping_add(offset) as usize,
                            &bytes,
                        )?;
                    } // C.SD
                    _ => return Err(IllegralInstruction),
                }
            }
            0b01 => {
                let (rd, imm) = extract_ci_type(self.c_instruction);

                let funct3 = self.c_instruction >> 13;

                // funct3
                match funct3 {
                    0b000 => {
                        if rd == 0 {
                            // C.NOP
                            // imm == 0の場合はNOPで以外のときはHINT？
                            // 一旦HINTは無視して実装する。
                        } else {
                            // C.ADDI
                            if imm == 0 {
                                panic!("Error: Ths imm of C.ADDI is not zero.");
                            }

                            self.write_reg(
                                Register::X(rd),
                                self.read_reg(Register::X(rd))?
                                    .wrapping_add(sign_extend(5, imm)),
                            )?;
                        }
                    }
                    0b001 => {
                        if rd != 0 {
                            self.write_reg(
                                Register::X(rd),
                                sign_extend(
                                    31,
                                    self.read_reg(Register::X(rd))?
                                        .wrapping_add(sign_extend(5, imm))
                                        & 0xffffffff,
                                ),
                            )?;
                        } else {
                            // rd=0は予約済み
                            return Err(IllegralInstruction);
                        }
                    } // C.ADDIW
                    0b010 => {
                        if rd != 0 {
                            self.write_reg(Register::X(rd), sign_extend(5, imm))?;
                        } else {
                            // rd=0の場合はHINTsをエンコードするらしい。
                        }
                    } // C.LI
                    0b011 => {
                        if rd == 0 {
                            panic!("Error: x0 is not zero with op=0b01 funct3=0b011.");
                        } else if rd == 2 {
                            // C.ADDI16SP
                            let nzimm = ((imm << 4) & 0x200)
                                | ((imm << 6) & 0x180)
                                | ((imm << 3) & 0x40)
                                | ((imm << 5) & 0x20)
                                | (imm & 0x10);

                            self.write_reg(
                                Register::X(2),
                                self.read_reg(Register::X(2))?
                                    .wrapping_add(sign_extend(9, nzimm)),
                            )?;
                        } else {
                            // C.LUI
                            if imm == 0 {
                                panic!("Error: Ths imm of C.LUI is not zero.");
                            }

                            let nzimm = imm << 12;

                            self.write_reg(Register::X(rd), sign_extend(17, nzimm))?;
                        }
                    }
                    0b100 => {
                        let funct2 = rd >> 3;
                        let rd = convert_from_c_reg_to_i(rd as u16 & 0x7);

                        match funct2 {
                            0 => {
                                if imm != 0 {
                                    self.write_reg(
                                        Register::X(rd),
                                        self.read_reg(Register::X(rd))? >> imm,
                                    )?;
                                } else {
                                    // imm=0の場合はHINTsをエンコードするらしい。
                                }
                            } // C.SRLI
                            0b01 => {
                                if imm != 0 {
                                    self.write_reg(
                                        Register::X(rd),
                                        sign_extend(
                                            63 - imm as u8,
                                            self.read_reg(Register::X(rd))? >> imm,
                                        ),
                                    )?;
                                } else {
                                    // imm=0の場合はHINTsをエンコードするらしい。
                                }
                            } // C.SRAI
                            0b10 => {
                                self.write_reg(
                                    Register::X(rd),
                                    self.read_reg(Register::X(rd))? & sign_extend(5, imm),
                                )?;
                            } // C.ANDI
                            0b11 => {
                                let rs2 = convert_from_c_reg_to_i(imm as u16 & 0x7);

                                // funct6[2], funct2
                                match (imm >> 5, (imm >> 3) & 0x3) {
                                    (0, 0) => {
                                        self.write_reg(
                                            Register::X(rd),
                                            self.read_reg(Register::X(rd))?
                                                .wrapping_sub(self.read_reg(Register::X(rs2))?),
                                        )?;
                                    } // C.SUB
                                    (0, 0b01) => {
                                        self.write_reg(
                                            Register::X(rd),
                                            self.read_reg(Register::X(rd))?
                                                ^ self.read_reg(Register::X(rs2))?,
                                        )?;
                                    } // C.XOR
                                    (0, 0b10) => {
                                        self.write_reg(
                                            Register::X(rd),
                                            self.read_reg(Register::X(rd))?
                                                | self.read_reg(Register::X(rs2))?,
                                        )?;
                                    } // C.OR
                                    (0, 0b11) => {
                                        self.write_reg(
                                            Register::X(rd),
                                            self.read_reg(Register::X(rd))?
                                                & self.read_reg(Register::X(rs2))?,
                                        )?;
                                    } // C.AND
                                    (0b1, 0) => {
                                        self.write_reg(
                                            Register::X(rd),
                                            sign_extend(
                                                31,
                                                self.read_reg(Register::X(rd))?
                                                    .wrapping_sub(self.read_reg(Register::X(rs2))?)
                                                    & 0xffffffff,
                                            ),
                                        )?;
                                    } // C.SUBW
                                    (0b1, 0b01) => {
                                        self.write_reg(
                                            Register::X(rd),
                                            sign_extend(
                                                31,
                                                self.read_reg(Register::X(rd))?
                                                    .wrapping_add(self.read_reg(Register::X(rs2))?)
                                                    & 0xffffffff,
                                            ),
                                        )?;
                                    } // C.ADDW
                                    _ => return Err(IllegralInstruction),
                                }
                            }
                            _ => return Err(IllegralInstruction),
                        }
                    }
                    0b101 => {
                        let imm = (self.c_instruction >> 1) & 0xffe;
                        let offset = (imm & 0xb40)
                            | ((imm << 3) & 0x400)
                            | ((imm << 2) & 0x80)
                            | ((imm << 4) & 0x20)
                            | ((imm >> 6) & 0x10)
                            | ((imm >> 1) & 0xe);

                        self.write_reg(
                            Register::Pc,
                            self.read_reg(Register::Pc)?
                                .wrapping_add(sign_extend(11, offset as u64)),
                        )?;

                        return Ok(EmulatorFlag::Jump);
                    } // C.J
                    0b110 | 0b111 => {
                        let (rs1, imm) = extract_cb_type(self.c_instruction);
                        let offset = ((imm << 1) & 0x100)
                            | ((imm << 3) & 0xc0)
                            | ((imm << 5) & 0x20)
                            | ((imm >> 2) & 0x18)
                            | (imm & 0x6);

                        let rs1 = self.read_reg(Register::X(rs1))?;

                        // C.BEQZ or C.BNEZ
                        if (funct3 == 0b110 && rs1 == 0) || (funct3 == 0b111 && rs1 != 0) {
                            self.write_reg(
                                Register::Pc,
                                self.read_reg(Register::Pc)?
                                    .wrapping_add(sign_extend(8, offset)),
                            )?;
                            return Ok(EmulatorFlag::Jump);
                        }
                    }
                    _ => return Err(IllegralInstruction),
                }
            }
            0b10 => {
                let (rd, imm) = extract_ci_type(self.c_instruction);

                match self.c_instruction >> 13 {
                    0 => {
                        if rd != 0 && imm != 0 {
                            self.write_reg(
                                Register::X(rd),
                                self.read_reg(Register::X(rd))? << imm,
                            )?;
                        } else {
                            // rd=0またはimm=0の場合はHINTsをエンコードするらしい。
                        }
                    } // C.SLLI
                    0b010 => {
                        if rd == 0 {
                            // rd=0は予約済み
                            return Err(IllegralInstruction);
                        }

                        let offset = ((imm << 6) & 0xc0) | (imm & 0x3c);

                        let bytes = self.read_memory::<4>(
                            self.read_reg(Register::X(2))?.wrapping_add(offset) as usize,
                        )?;

                        self.write_reg(
                            Register::X(rd),
                            sign_extend(31, u32::from_le_bytes(bytes) as u64),
                        )?;
                    } // C.LWSP
                    0b011 => {
                        if rd == 0 {
                            // rd=0は予約済み
                            return Err(IllegralInstruction);
                        }

                        let offset = ((imm << 6) & 0xc0) | (imm & 0x3c);

                        self.write_reg(
                            Register::X(rd),
                            u64::from_le_bytes(self.read_memory::<8>(
                                self.read_reg(Register::X(2))?.wrapping_add(offset) as usize,
                            )?),
                        )?;
                    } // C.LDSP
                    0b100 => {
                        let rs2 = imm as u8 & 0x1f;
                        let funct4 = imm >> 5;

                        if rd == 0 && rs2 == 0 && funct4 == 1 {
                            // C.EBREAK
                            return Err(IllegralInstruction);
                        }

                        if rs2 == 0 {
                            // C.JR & C.JALR
                            if rd == 0 {
                                // rs1=0は予約済み
                                return Err(IllegralInstruction);
                            }

                            if funct4 == 1 {
                                // C.JALR
                                self.write_reg(
                                    Register::X(1),
                                    self.read_reg(Register::Pc)?.wrapping_add(2),
                                )?;
                            }

                            self.write_reg(Register::Pc, self.read_reg(Register::X(rd))? & !1)?;
                            return Ok(EmulatorFlag::Jump);
                        }

                        if funct4 == 0 {
                            // C.MV
                            if rd == 0 {
                                // rd=0の場合はHINTsをエンコードするらしい。
                            } else {
                                self.write_reg(Register::X(rd), self.read_reg(Register::X(rs2))?)?;
                            }
                        } else {
                            // C.ADD
                            if rd == 0 {
                                // rd=0の場合はHINTsをエンコードするらしい。
                            } else {
                                self.write_reg(
                                    Register::X(rd),
                                    self.read_reg(Register::X(rd))?
                                        .wrapping_add(self.read_reg(Register::X(rs2))?),
                                )?;
                            }
                        }
                    }
                    0b110 => {
                        let rs2 = (imm & 0x1f) as u8;
                        let imm = (imm & 0x20) | rd as u64;
                        let offset = ((imm << 6) & 0xc0) | (imm & 0x3c);

                        let bytes = (self.read_reg(Register::X(rs2))? as u32).to_le_bytes();

                        self.write_memory(
                            self.read_reg(Register::X(2))?.wrapping_add(offset) as usize,
                            &bytes,
                        )?;
                    } // C.SWSP
                    0b111 => {
                        let rs2 = (imm & 0x1f) as u8;
                        let imm = (imm & 0x20) | rd as u64;
                        let offset = ((imm << 6) & 0x7) | (imm & 0x38);

                        self.write_memory(
                            self.read_reg(Register::X(2))?.wrapping_add(offset) as usize,
                            &self.read_reg(Register::X(rs2))?.to_le_bytes(),
                        )?;
                    } // C.SDSP
                    _ => return Err(IllegralInstruction),
                }
            }
            _ => return Err(IllegralInstruction),
        }

        Ok(EmulatorFlag::ExeC)
    }

    // 命令を格納するバイト列から実行する命令を判定し命令を実行する関数
    // 例外が発生した場合は即座にErrに起こった例外に対応するException型の値を返す。
    fn exec(&mut self) -> Result<EmulatorFlag> {
        // instruction == 0の場合は不正な命令である。
        if self.instruction == 0 {
            return Err(IllegralInstruction);
        }

        // C拡張が有効の場合かつC拡張の命令の場合はc_execを実行する
        if self.is_c_extension_enabled() && self.instruction & 0x3 < 3 {
            return self.c_exec();
        } else {
            self.c_instruction = 0;
        }

        // instruction & 0x3 != 3以外ならRV32もしくはRV64ではない可能性がある。
        if self.instruction & 0x3 != 3 {
            return Err(IllegralInstruction);
        }

        let op = (self.instruction >> 2) & 0x1f;
        let funct3 = (self.instruction >> 12) & 0x7;

        match op {
            0b00000 => {
                let (rd, rs1, imm) = extract_i_type(self.instruction);

                match funct3 {
                    0b000 => {
                        let bytes = self.read_memory::<1>(
                            self.read_reg(Register::X(rs1))?
                                .wrapping_add(sign_extend(11, imm))
                                as usize,
                        )?;

                        self.write_reg(
                            Register::X(rd),
                            sign_extend(7, u8::from_le_bytes(bytes) as u64),
                        )?;
                    } // LB
                    0b001 => {
                        let bytes = self.read_memory::<2>(
                            self.read_reg(Register::X(rs1))?
                                .wrapping_add(sign_extend(11, imm))
                                as usize,
                        )?;

                        self.write_reg(
                            Register::X(rd),
                            sign_extend(15, u16::from_le_bytes(bytes) as u64),
                        )?;
                    } // LH
                    0b010 => {
                        let bytes = self.read_memory::<4>(
                            self.read_reg(Register::X(rs1))?
                                .wrapping_add(sign_extend(11, imm))
                                as usize,
                        )?;

                        self.write_reg(
                            Register::X(rd),
                            sign_extend(31, u32::from_le_bytes(bytes) as u64),
                        )?;
                    } // LW
                    0b011 => {
                        let bytes = self.read_memory::<8>(
                            self.read_reg(Register::X(rs1))?
                                .wrapping_add(sign_extend(11, imm))
                                as usize,
                        )?;

                        self.write_reg(Register::X(rd), u64::from_le_bytes(bytes))?;
                    } // LD
                    0b100 => {
                        let bytes = self.read_memory::<1>(
                            self.read_reg(Register::X(rs1))?
                                .wrapping_add(sign_extend(11, imm))
                                as usize,
                        )?;

                        self.write_reg(Register::X(rd), u8::from_le_bytes(bytes) as u64)?;
                    } // LBU
                    0b101 => {
                        let bytes = self.read_memory::<2>(
                            self.read_reg(Register::X(rs1))?
                                .wrapping_add(sign_extend(11, imm))
                                as usize,
                        )?;

                        self.write_reg(Register::X(rd), u16::from_le_bytes(bytes) as u64)?;
                    } // LHU
                    0b110 => {
                        let bytes = self.read_memory::<4>(
                            self.read_reg(Register::X(rs1))?
                                .wrapping_add(sign_extend(11, imm))
                                as usize,
                        )?;

                        self.write_reg(Register::X(rd), u32::from_le_bytes(bytes) as u64)?;
                    } // LHU
                    _ => return Err(IllegralInstruction),
                }
            }
            0b00011 => {
                // 並行処理系の工夫する構造はないので作るまでは実装しない。
                eprintln!("[warning]: fence may not work properly.");

                match self.instruction {
                    0x8330000f | 0x0100000f => {
                        // FENCE.TSO PAUSEは実装していない
                        return Err(IllegralInstruction);
                    } // FENCE.TSO PAUSE
                    _ => {} //fence
                }
            }
            0b00100 => {
                let (rd, rs1, imm) = extract_i_type(self.instruction);

                match (funct3, imm >> 6) {
                    (0b000, _) => self.write_reg(
                        Register::X(rd),
                        self.read_reg(Register::X(rs1))?
                            .wrapping_add(sign_extend(11, imm)),
                    )?, //ADDI
                    (0b001, 0b000000) => {
                        self.write_reg(
                            Register::X(rd),
                            self.read_reg(Register::X(rs1))? << (imm & 0x3f),
                        )?;
                    } // SLLI
                    (0b010, _) => {
                        self.write_reg(
                            Register::X(rd),
                            if sign_extend(11, imm) as i64 > self.read_reg(Register::X(rs1))? as i64
                            {
                                1
                            } else {
                                0
                            },
                        )?;
                    } // SLTI
                    (0b011, _) => {
                        self.write_reg(
                            Register::X(rd),
                            if sign_extend(11, imm) > self.read_reg(Register::X(rs1))? {
                                1
                            } else {
                                0
                            },
                        )?;
                    } // SLTIU
                    (0b100, _) => {
                        self.write_reg(
                            Register::X(rd),
                            self.read_reg(Register::X(rs1))? ^ sign_extend(11, imm),
                        )?;
                    } // XORI
                    (0b101, 0b000000) => {
                        self.write_reg(
                            Register::X(rd),
                            self.read_reg(Register::X(rs1))? >> (imm & 0x3f),
                        )?;
                    } // SRLI
                    (0b101, 0b010000) => {
                        self.write_reg(
                            Register::X(rd),
                            sign_extend(
                                63 - (imm as u8 & 0x3f),
                                self.read_reg(Register::X(rs1))? >> (imm & 0x3f),
                            ),
                        )?;
                    } // SRAI
                    (0b110, _) => self.write_reg(
                        Register::X(rd),
                        self.read_reg(Register::X(rs1))? | sign_extend(11, imm),
                    )?, // ORI
                    (0b111, _) => self.write_reg(
                        Register::X(rd),
                        self.read_reg(Register::X(rs1))? & sign_extend(11, imm),
                    )?, // ANDI
                    _ => return Err(IllegralInstruction),
                };
            }
            0b00101 => {
                let (rd, imm) = extract_u_type(self.instruction);

                self.write_reg(
                    Register::X(rd),
                    self.read_reg(Register::Pc)?
                        .wrapping_add(sign_extend(31, imm)),
                )?;
            } // AUIPC
            0b00110 => {
                let (rd, rs1, imm) = extract_i_type(self.instruction);

                match (funct3, self.instruction >> 26) {
                    (0b000, _) => {
                        self.write_reg(
                            Register::X(rd),
                            sign_extend(
                                31,
                                self.read_reg(Register::X(rs1))?
                                    .wrapping_add(sign_extend(11, imm))
                                    & 0xffffffff,
                            ),
                        )?;
                    } // ADDIW
                    (0b001, 0) => {
                        self.write_reg(
                            Register::X(rd),
                            sign_extend(
                                31,
                                (self.read_reg(Register::X(rs1))? << (imm & 0x1f)) & 0xffffffff,
                            ),
                        )?;
                    } // SLL
                    (0b101, 0) => {
                        self.write_reg(
                            Register::X(rd),
                            sign_extend(
                                31,
                                (self.read_reg(Register::X(rs1))? & 0xffffffff) >> (imm & 0x1f)
                                    & 0xffffffff,
                            ),
                        )?;
                    } // SRLIW
                    (0b101, 0b010000) => {
                        self.write_reg(
                            Register::X(rd),
                            sign_extend(
                                31 - (imm & 0x1f) as u8,
                                ((self.read_reg(Register::X(rs1))? & 0xffffffff) >> (imm & 0x1f))
                                    & 0xffffffff,
                            ),
                        )?;
                    } // SRAIW
                    _ => return Err(IllegralInstruction),
                }
            }
            0b01000 => {
                let (rs1, rs2, imm) = extract_s_type(self.instruction);

                // 簡単にリファクタできるかもしれない

                match funct3 {
                    0b000 => {
                        let bytes = (self.read_reg(Register::X(rs2))? as u8).to_le_bytes();

                        self.write_memory(
                            self.read_reg(Register::X(rs1))?
                                .wrapping_add(sign_extend(11, imm))
                                as usize,
                            &bytes,
                        )?;
                    } // SB
                    0b001 => {
                        let bytes = (self.read_reg(Register::X(rs2))? as u16).to_le_bytes();

                        self.write_memory(
                            self.read_reg(Register::X(rs1))?
                                .wrapping_add(sign_extend(11, imm))
                                as usize,
                            &bytes,
                        )?;
                    } // SH
                    0b010 => {
                        let bytes = (self.read_reg(Register::X(rs2))? as u32).to_le_bytes();

                        self.write_memory(
                            self.read_reg(Register::X(rs1))?
                                .wrapping_add(sign_extend(11, imm))
                                as usize,
                            &bytes,
                        )?;
                    } // SW
                    0b011 => {
                        let bytes = self.read_reg(Register::X(rs2))?.to_le_bytes();

                        self.write_memory(
                            self.read_reg(Register::X(rs1))?
                                .wrapping_add(sign_extend(11, imm))
                                as usize,
                            &bytes,
                        )?;
                    } // SD
                    _ => return Err(IllegralInstruction),
                }
            }
            0b01011 => {
                let (rd, rs1, rs2, funct7) = extract_r_type(self.instruction);

                let addr = self.read_reg(Register::X(rs1))? as usize;

                // PMAでアライメントは変更することができるらしい。

                match funct3 {
                    0b010 => {
                        // 32bit版の場合は4バイトアライメント
                        self.check_misaligned(addr as u64)?;

                        if funct7 >> 2 == 0b00011 {
                            // SC.W

                            if let Some(range) = self.pop_reserved_memory_range() {
                                // 予約領域が存在している場合

                                if range.0 <= addr && range.1 >= addr + 4 {
                                    // 予約領域内の場合はそのメモリ領域に書き込みを行い、rdに0を書き込む。
                                    self.write_memory(
                                        addr,
                                        &(self.read_reg(Register::X(rs2))? as u32).to_le_bytes(),
                                    )?;

                                    self.write_reg(Register::X(rd), 0)?;
                                } else {
                                    // 上の条件に当てはまらない場合はrdに1を書き込むことにする。
                                    self.write_reg(Register::X(rd), 1)?;
                                }
                            } else {
                                // ここで二回同じコードを書いているがif-let chainが使えるようになったら一つで済むようになる。
                                self.write_reg(Register::X(rd), 1)?;
                            }
                        } else {
                            let v = u32::from_le_bytes(self.read_memory::<4>(addr)?);

                            match funct7 >> 2 {
                                0 => self.write_memory(
                                    addr,
                                    &(v.wrapping_add(self.read_reg(Register::X(rs2))? as u32))
                                        .to_le_bytes(),
                                )?, // AMOADD.W
                                0b00001 => {
                                    self.write_memory(
                                        addr,
                                        &(self.read_reg(Register::X(rs2))? as u32).to_le_bytes(),
                                    )?;
                                    self.write_reg(Register::X(rs2), v as u64)?;
                                } // AMOSWAP.W
                                0b00010 => {
                                    self.write_reg(Register::X(rd), sign_extend(31, v as u64))?;
                                    self.push_reserved_memory_range((addr, addr + 4));
                                } // LR.W
                                // 0b00011 => {} // SC.W 上に実装済み
                                0b00100 => self.write_memory(
                                    addr,
                                    &(v ^ (self.read_reg(Register::X(rs2))? as u32)).to_le_bytes(),
                                )?,
                                0b01100 => self.write_memory(
                                    addr,
                                    &(v & (self.read_reg(Register::X(rs2))? as u32)).to_le_bytes(),
                                )?, // AMOAND.W
                                0b01000 => self.write_memory(
                                    addr,
                                    &(v | (self.read_reg(Register::X(rs2))? as u32)).to_le_bytes(),
                                )?,
                                0b10000 => {
                                    let rs2_val = self.read_reg(Register::X(rs2))? as u32;

                                    self.write_memory(
                                        addr,
                                        &(if rs2_val as i32 > v as i32 {
                                            v
                                        } else {
                                            rs2_val
                                        })
                                        .to_le_bytes(),
                                    )?;
                                } // AMOMIN.W
                                0b10100 => {
                                    let rs2_val = self.read_reg(Register::X(rs2))? as u32;

                                    self.write_memory(
                                        addr,
                                        &(if v as i32 > rs2_val as i32 {
                                            v
                                        } else {
                                            rs2_val
                                        })
                                        .to_le_bytes(),
                                    )?;
                                } // AMOMAX.W
                                0b11000 => self.write_memory(
                                    addr,
                                    &v.min(self.read_reg(Register::X(rs2))? as u32).to_le_bytes(),
                                )?, // AMOMINU.W
                                0b11100 => self.write_memory(
                                    addr,
                                    &v.max(self.read_reg(Register::X(rs2))? as u32).to_le_bytes(),
                                )?, // AMOMAXU.W
                                _ => return Err(IllegralInstruction),
                            }

                            self.write_reg(Register::X(rd), sign_extend(31, v as u64))?;
                        }
                    }
                    0b011 => {
                        // 64bit版の場合は8バイトアライメント
                        self.check_misaligned_nbyte_misaligned(addr as u64, 8)?;

                        let v = u64::from_le_bytes(self.read_memory::<8>(addr)?);

                        match funct7 >> 2 {
                            0 => self.write_memory(
                                addr,
                                &(v.wrapping_add(self.read_reg(Register::X(rs2))?)).to_le_bytes(),
                            )?, // AMOADD.D
                            0b00001 => {
                                self.write_memory(
                                    addr,
                                    &self.read_reg(Register::X(rs2))?.to_le_bytes(),
                                )?;
                                self.write_reg(Register::X(rs2), v)?;
                            } // AMOSWAP.D
                            // 0b0011 => {} SC.Dを作るときはSC.Wを参考にする。
                            0b00100 => self.write_memory(
                                addr,
                                &(v ^ self.read_reg(Register::X(rs2))?).to_le_bytes(),
                            )?, // AMOXOR.D
                            0b01100 => self.write_memory(
                                addr,
                                &(v & self.read_reg(Register::X(rs2))?).to_le_bytes(),
                            )?, // AMOAND.D
                            0b01000 => self.write_memory(
                                addr,
                                &(v | self.read_reg(Register::X(rs2))?).to_le_bytes(),
                            )?, // AMOOR.D
                            0b10000 => {
                                let rs2_val = self.read_reg(Register::X(rs2))?;

                                self.write_memory(
                                    addr,
                                    &(if rs2_val as i64 > v as i64 {
                                        v
                                    } else {
                                        rs2_val
                                    })
                                    .to_le_bytes(),
                                )?;
                            } // AMOMIN.D
                            0b10100 => {
                                let rs2_val = self.read_reg(Register::X(rs2))?;

                                self.write_memory(
                                    addr,
                                    &(if v as i64 > rs2_val as i64 {
                                        v
                                    } else {
                                        rs2_val
                                    })
                                    .to_le_bytes(),
                                )?;
                            } // AMOMAX.D
                            0b11000 => self.write_memory(
                                addr,
                                &v.min(self.read_reg(Register::X(rs2))?).to_le_bytes(),
                            )?,
                            0b11100 => self.write_memory(
                                addr,
                                &v.max(self.read_reg(Register::X(rs2))?).to_le_bytes(),
                            )?, // AMOMAXU.D
                            _ => return Err(IllegralInstruction),
                        }

                        self.write_reg(Register::X(rd), v)?;
                    }
                    _ => return Err(IllegralInstruction),
                }
            }
            0b01100 => {
                let (rd, rs1, rs2, funct7) = extract_r_type(self.instruction);

                // rd == 0のときにスキップしても良さそうではある。

                match (funct3, funct7) {
                    (0, 0) => self.write_reg(
                        Register::X(rd),
                        self.read_reg(Register::X(rs1))?
                            .wrapping_add(self.read_reg(Register::X(rs2))?),
                    )?, // ADD
                    (0, 0b0000001) => self.write_reg(
                        Register::X(rd),
                        self.read_reg(Register::X(rs1))?
                            .wrapping_mul(self.read_reg(Register::X(rs2))?),
                    )?, // MUL
                    (0, 0b0100000) => self.write_reg(
                        Register::X(rd),
                        self.read_reg(Register::X(rs1))?
                            .wrapping_sub(self.read_reg(Register::X(rs2))?),
                    )?, // SUB
                    (0b001, 0) => self.write_reg(
                        Register::X(rd),
                        self.read_reg(Register::X(rs1))?
                            << (self.read_reg(Register::X(rs2))? & 0x3f),
                    )?, // SLL
                    (0b001, 0b0000001) => {
                        let rs1 = sign_extend_128bit(63, self.read_reg(Register::X(rs1))? as u128);
                        let rs2 = sign_extend_128bit(63, self.read_reg(Register::X(rs2))? as u128);

                        self.write_reg(
                            Register::X(rd),
                            (((rs1 as i128) * (rs2 as i128)) >> 64) as u64,
                        )?;
                    } // MULH
                    (0b010, 0) => self.write_reg(
                        Register::X(rd),
                        if self.read_reg(Register::X(rs2))? as i64
                            > self.read_reg(Register::X(rs1))? as i64
                        {
                            1
                        } else {
                            0
                        },
                    )?, // SLT
                    (0b010, 0b0000001) => {
                        let rs1 = sign_extend_128bit(63, self.read_reg(Register::X(rs1))? as u128);
                        let rs2 = self.read_reg(Register::X(rs2))? as u128;

                        self.write_reg(Register::X(rd), (rs1.wrapping_mul(rs2) >> 64) as u64)?;
                    } // MULH
                    (0b011, 0) => self.write_reg(
                        Register::X(rd),
                        if self.read_reg(Register::X(rs2))? > self.read_reg(Register::X(rs1))? {
                            1
                        } else {
                            0
                        },
                    )?, // SLTU
                    (0b011, 0b0000001) => {
                        let rs1 = self.read_reg(Register::X(rs1))? as u128;
                        let rs2 = self.read_reg(Register::X(rs2))? as u128;

                        self.write_reg(Register::X(rd), (rs1.wrapping_mul(rs2) >> 64) as u64)?;
                    } // MULHU
                    (0b100, 0) => {
                        self.write_reg(
                            Register::X(rd),
                            self.read_reg(Register::X(rs1))? ^ self.read_reg(Register::X(rs2))?,
                        )?;
                    } // XOR
                    (0b100, 0b0000001) => {
                        let rs1 = self.read_reg(Register::X(rs1))?;
                        let rs2 = self.read_reg(Register::X(rs2))?;

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
                        )?;
                    } // DIV
                    (0b101, 0) => {
                        let shift = self.read_reg(Register::X(rs2))? & 0x3f;

                        self.write_reg(Register::X(rd), self.read_reg(Register::X(rs1))? >> shift)?;
                    } // SRL
                    (0b101, 0b0000001) => {
                        let rs1 = self.read_reg(Register::X(rs1))?;
                        let rs2 = self.read_reg(Register::X(rs2))?;

                        self.write_reg(
                            Register::X(rd),
                            if rs2 == 0 { u64::MAX } else { rs1 / rs2 },
                        )?;
                    } // DIVU
                    (0b101, 0b0100000) => {
                        let shift = self.read_reg(Register::X(rs2))? & 0x3f;

                        self.write_reg(
                            Register::X(rd),
                            sign_extend(
                                63 - shift as u8,
                                self.read_reg(Register::X(rs1))? >> shift,
                            ),
                        )?;
                    } // SRA
                    (0b110, 0) => self.write_reg(
                        Register::X(rd),
                        self.read_reg(Register::X(rs1))? | self.read_reg(Register::X(rs2))?,
                    )?, // OR
                    (0b110, 0b0000001) => {
                        let rs1 = self.read_reg(Register::X(rs1))?;
                        let rs2 = self.read_reg(Register::X(rs2))?;

                        self.write_reg(
                            Register::X(rd),
                            if rs1 == 1 << 63 && rs2 == !0 {
                                0
                            } else if rs2 == 0 {
                                rs1
                            } else {
                                (rs1 as i64 % rs2 as i64) as u64
                            },
                        )?
                    } // REM
                    (0b111, 0) => self.write_reg(
                        Register::X(rd),
                        self.read_reg(Register::X(rs1))? & self.read_reg(Register::X(rs2))?,
                    )?, // AND
                    (0b111, 0b0000001) => {
                        let rs1 = self.read_reg(Register::X(rs1))?;
                        let rs2 = self.read_reg(Register::X(rs2))?;

                        self.write_reg(Register::X(rd), if rs2 == 0 { rs1 } else { rs1 % rs2 })?;
                    } // REMU
                    _ => return Err(IllegralInstruction),
                }
            }
            0b01101 => {
                let (rd, imm) = extract_u_type(self.instruction);

                self.write_reg(Register::X(rd), sign_extend(31, imm))?;
            } // LUI
            0b01110 => {
                let (rd, rs1, rs2, funct7) = extract_r_type(self.instruction);

                match (funct3, funct7) {
                    (0b000, 0) => {
                        self.write_reg(
                            Register::X(rd),
                            sign_extend(
                                31,
                                self.read_reg(Register::X(rs1))?
                                    .wrapping_add(self.read_reg(Register::X(rs2))?)
                                    & 0xffffffff,
                            ),
                        )?;
                    } // ADDW
                    (0b000, 0b0000001) => {
                        let rs1 = self.read_reg(Register::X(rs1))? & 0xffffffff;
                        let rs2 = self.read_reg(Register::X(rs2))? & 0xffffffff;

                        self.write_reg(Register::X(rd), sign_extend(31, (rs1 * rs2) & 0xffffffff))?;
                    } // MULW
                    (0b000, 0b0100000) => {
                        self.write_reg(
                            Register::X(rd),
                            sign_extend(
                                31,
                                self.read_reg(Register::X(rs1))?
                                    .wrapping_sub(self.read_reg(Register::X(rs2))?)
                                    & 0xffffffff,
                            ),
                        )?;
                    } // ADDW
                    (0b001, 0) => {
                        self.write_reg(
                            Register::X(rd),
                            sign_extend(
                                31,
                                (self.read_reg(Register::X(rs1))?
                                    << (self.read_reg(Register::X(rs2))? & 0x1f))
                                    & 0xffffffff,
                            ),
                        )?;
                    } // SLLW
                    (0b100, 0b0000001) => {
                        let rs1 = self.read_reg(Register::X(rs1))? as i32;
                        let rs2 = self.read_reg(Register::X(rs2))? as i32;

                        self.write_reg(
                            Register::X(rd),
                            if rs1 == i32::MIN && rs2 == !0 {
                                rs1 as u64
                            } else if rs2 == 0 {
                                u64::MAX
                            } else {
                                (rs1 / rs2) as u64
                            },
                        )?;
                    } // SLLW
                    (0b101, 0) => {
                        let shift = self.read_reg(Register::X(rs2))? & 0x1f;
                        self.write_reg(
                            Register::X(rd),
                            sign_extend(
                                31,
                                ((self.read_reg(Register::X(rs1))? & 0xffffffff) >> shift)
                                    & 0xffffffff,
                            ),
                        )?;
                    } // SRLW
                    (0b101, 0b0000001) => {
                        let rs1 = self.read_reg(Register::X(rs1))?;
                        let rs2 = self.read_reg(Register::X(rs2))?;

                        self.write_reg(
                            Register::X(rd),
                            if rs2 == 0 {
                                u64::MAX
                            } else {
                                sign_extend(31, (rs1 / rs2) & 0xffffffff)
                            },
                        )?;
                    } // DIVUW
                    (0b101, 0b0100000) => {
                        let shift = self.read_reg(Register::X(rs2))? & 0x1f;
                        self.write_reg(
                            Register::X(rd),
                            sign_extend(
                                31 - shift as u8,
                                ((self.read_reg(Register::X(rs1))? & 0xffffffff) >> shift)
                                    & 0xffffffff,
                            ),
                        )?;
                    } // SRAW
                    (0b110, 0b0000001) => {
                        let rs1 = self.read_reg(Register::X(rs1))? & 0xffffffff;
                        let rs2 = self.read_reg(Register::X(rs2))? & 0xffffffff;

                        self.write_reg(
                            Register::X(rd),
                            if rs1 == 1 << 31 && rs2 == 0xffffffff {
                                0
                            } else if rs2 == 0 {
                                sign_extend(31, rs1)
                            } else {
                                (rs1 as i32 % rs2 as i32) as u64
                            },
                        )?;
                    } // REMW
                    (0b111, 0b0000001) => {
                        let rs1 = self.read_reg(Register::X(rs1))? & 0xffffffff;
                        let rs2 = self.read_reg(Register::X(rs2))? & 0xffffffff;

                        self.write_reg(
                            Register::X(rd),
                            sign_extend(31, if rs2 == 0 { rs1 } else { rs1 % rs2 }),
                        )?;
                    } // REMUW
                    _ => return Err(IllegralInstruction),
                }
            }
            0b11000 => {
                let (rs1, rs2, imm) = extract_b_type(self.instruction);
                let offset = sign_extend(12, imm);

                let mut flag = false;

                match funct3 {
                    0b000 => {
                        flag =
                            self.read_reg(Register::X(rs1))? == self.read_reg(Register::X(rs2))?;
                    } // BEQ
                    0b001 => {
                        flag =
                            self.read_reg(Register::X(rs1))? != self.read_reg(Register::X(rs2))?;
                    } // BNE
                    0b100 => {
                        flag = self.read_reg(Register::X(rs2))? as i64
                            > self.read_reg(Register::X(rs1))? as i64;
                    } // BLT
                    0b101 => {
                        flag = self.read_reg(Register::X(rs1))? as i64
                            >= self.read_reg(Register::X(rs2))? as i64;
                    } // BGE
                    0b110 => {
                        flag =
                            self.read_reg(Register::X(rs2))? > self.read_reg(Register::X(rs1))?;
                    } // BLT
                    0b111 => {
                        flag =
                            self.read_reg(Register::X(rs1))? >= self.read_reg(Register::X(rs2))?;
                    } // BGEU
                    _ => return Err(IllegralInstruction),
                };

                if flag {
                    let dst = self.read_reg(Register::Pc)?.wrapping_add(offset);
                    self.check_misaligned(dst)?;

                    self.write_reg(Register::Pc, dst)?;
                    return Ok(EmulatorFlag::Jump);
                }
            }
            0b11001 => {
                let (rd, rs1, imm) = extract_i_type(self.instruction);
                let offset = sign_extend(11, imm);

                let pc = self.read_reg(Register::Pc)?;
                let dst = self.read_reg(Register::X(rs1))?.wrapping_add(offset) & !1;

                self.check_misaligned(dst)?;

                self.write_reg(Register::X(rd), pc + 4)?;
                self.write_reg(Register::Pc, dst)?;
                return Ok(EmulatorFlag::Jump);
            } // JALR
            0b11011 => {
                let (rd, imm) = extract_j_type(self.instruction);
                let offset = sign_extend(20, imm);

                let pc = self.read_reg(Register::Pc)?;
                let dst = pc.wrapping_add(offset);

                // ターゲットアドレスが4byteでアライメントされていない場合はアライメントの例外を起こす。

                self.check_misaligned(dst)?;

                self.write_reg(Register::X(rd), pc + 4)?;
                self.write_reg(Register::Pc, dst)?;
                return Ok(EmulatorFlag::Jump);
            } //JAL
            0b11100 => {
                let (rd, rs1, imm) = extract_i_type(self.instruction);

                // 実装するときは非権限の方のCSRの章を読む。
                // rd==0のときとかrs1==0のときに読み書きしない場合等が記載されている。
                // IllegralInstructionのときはここの命令が実装されていないorCSRの方で実装されていないかを疑う。
                // CSRの方のいくつかの例外はVirtual Instructionでもいいらしい。

                match funct3 {
                    0b000 => {
                        match self.instruction {
                            0x00000073 => match self.current_priv {
                                Priv::M => return Err(EnvironmentCallFromMMode),
                                Priv::S => return Err(EnvironmentCallFromSMode),
                                Priv::U => return Err(EnvironmentCallFromUMode),
                            }, //ECALL
                            0x30200073 => match self.current_priv {
                                Priv::M => {
                                    let mstatus = self.read_raw_csr(CSR_MSTATUS)?;

                                    let mpp = (mstatus & CSR_MSTATUS_MPP_MASK) >> 11;
                                    let mpie = (mstatus & CSR_MSTATUS_MPIE_MASK) >> 7;

                                    let new_mstatus = (mstatus
                                        & !CSR_MSTATUS_MIE_MASK
                                        & !CSR_MSTATUS_MPP_MASK
                                        & !(CSR_MSTATUS_MPIE_MASK))
                                        | (mpie << 3)
                                        | (1 << 7)
                                        | ((Priv::U as u64) << 11);

                                    self.write_csr(CSR_MSTATUS, new_mstatus)?;
                                    self.write_reg(Register::Pc, self.read_csr(CSR_MEPC)?)?;
                                    self.current_priv = Priv::from(mpp);

                                    return Ok(EmulatorFlag::Jump);
                                } // MRET
                                _ => {
                                    // Mモード以外で呼び出された場合は実装していない。
                                    return Err(IllegralInstruction);
                                }
                            }, // xRET
                            _ => return Err(IllegralInstruction),
                        }
                    }
                    0b001 => {
                        let csr = if rd != 0 { self.read_csr(imm)? } else { 0 };
                        self.write_csr(imm, self.read_reg(Register::X(rs1))?)?;

                        if rd != 0 {
                            self.write_reg(Register::X(rd), csr)?;
                        }
                    } // CSRRW
                    0b010 => {
                        let csr = self.read_csr(imm)?;
                        let rs1 = self.read_reg(Register::X(rs1))?;

                        self.write_reg(Register::X(rd), csr)?;

                        if rs1 != 0 {
                            self.write_csr(imm, csr | rs1)?;
                        }
                    } // CSRRS
                    0b011 => {
                        let csr = self.read_csr(imm)?;
                        let rs1 = self.read_reg(Register::X(rs1))?;

                        self.write_reg(Register::X(rd), csr)?;

                        if rs1 != 0 {
                            self.write_csr(imm, csr ^ rs1)?;
                        }
                    } // CSRRC
                    0b101 => {
                        let csr = if rd != 0 { self.read_csr(imm)? } else { 0 };

                        self.write_csr(imm, rs1 as u64)?;

                        if rd != 0 {
                            self.write_reg(Register::X(rd), csr)?;
                        }
                    } // CSRRWI
                    0b110 => {
                        let csr = self.read_csr(imm)?;

                        self.write_reg(Register::X(rd), csr)?;

                        if rs1 != 0 {
                            self.write_csr(imm, csr | rs1 as u64)?;
                        }
                    } // CSRRSI
                    0b111 => {
                        let csr = self.read_csr(imm)?;

                        self.write_reg(Register::X(rd), csr)?;

                        if rs1 != 0 {
                            self.write_csr(imm, csr ^ rs1 as u64)?;
                        }
                    } // CSRRCI
                    _ => return Err(IllegralInstruction),
                }
            }
            _ => return Err(IllegralInstruction),
        }

        Ok(EmulatorFlag::Common)
    }

    // pcを一つ(4byte)進ませる関数
    // run以外で呼ばれずに、例外を起こす可能性がないのでread_regを呼び出していない。
    // run以外から呼んではいけない。
    fn progress_pc(&mut self) {
        self.pc += 4;
    }

    // C拡張を実行されたあとにpcを進ませる関数
    fn c_progress_pc(&mut self) {
        self.pc += 2;
    }

    pub fn run(&mut self) {
        loop {
            if self.riscv_tests_finished {
                break;
            }

            eprintln!("PC: 0x{:016x}", self.pc,);
            self.fetch();

            if self.pc >= 0x292 && self.pc <= 0x296 {
                self.show_regs();
            }

            match self.exec() {
                Err(e) => {
                    use crate::exception::Exception::*;

                    if self.read_raw_csr(CSR_MEDELEG).unwrap() != 0
                        || self.read_raw_csr(CSR_MIDELEG).unwrap() != 0
                    {
                        // 移譲の実装はまだ行わない。
                        panic!("Error: mideleg or medeleg is not implemented.");
                    }

                    // 移譲を行った場合はSモード用のCSRを使用する。

                    if e as u64 >> 63 == 1 {
                        // 割り込みの実装はまだ行わない。
                        // mstatusやmie,mipの実装はまだ
                        panic!("Error: interrupts are not implemented.");
                    } else {
                        // 例外の処理

                        let mpp = self.current_priv as u64;
                        self.current_priv = Priv::M;

                        let mstatus = self.read_raw_csr(CSR_MSTATUS).unwrap();
                        let mie = (mstatus & CSR_MSTATUS_MIE_MASK) >> 3;
                        self.write_csr(CSR_MEPC, self.pc).unwrap();
                        self.write_csr(
                            CSR_MSTATUS,
                            (mstatus
                                & !CSR_MSTATUS_MPP_MASK
                                & !CSR_MSTATUS_MPIE_MASK
                                & !CSR_MSTATUS_MIE_MASK)
                                | (mpp << 11)
                                | (mie << 7),
                        )
                        .unwrap();

                        self.write_csr(CSR_MCAUSE, e as u64).unwrap();
                    }

                    match e {
                        EnvironmentCallFromMMode
                        | EnvironmentCallFromUMode
                        | InstructionAddressMissaligned => {
                            // 同期例外の場合はモードにかかわらずpcにBASEを設定する。
                            // 多分ハンドラがmcauseの値からどの処理を行うかを判定する感じかな。
                            self.exception_direct_jump();
                        }
                        IllegralInstruction => {
                            // 命令が0、C拡張が有効でなく、C命令の場合はとりあえず不正命令の処理を行う
                            // それ場合は実装していない命令の可能性があるので一応panicにする。
                            if (self.c_instruction == 0 && self.instruction == 0)
                                || (!self.is_c_extension_enabled() && self.instruction & 0x3 != 3)
                            {
                                let mtval = if self.c_instruction != 0 {
                                    self.c_instruction as u32
                                } else {
                                    self.instruction
                                };
                                self.write_csr(CSR_MTVAL, mtval as u64).unwrap();

                                self.exception_direct_jump();
                            } else {
                                let op = self.instruction & 0x7f;
                                let funct3 = (self.instruction >> 12) & 0x7;
                                panic!(
                        "instruction: 0x{:08x} op: 0b{:07b} funct3: 0b{:03b}\nException: {:?}",
                        self.instruction, op, funct3, e
                    );
                            }
                        }
                        _ => {
                            let op = self.instruction & 0x7f;
                            let funct3 = (self.instruction >> 12) & 0x7;
                            panic!(
                        "instruction: 0x{:08x} op: 0b{:07b} funct3: 0b{:03b}\nException: {:?}",
                        self.instruction, op, funct3, e
                    );
                        }
                    }
                }
                Ok(flag) => {
                    use EmulatorFlag::*;

                    match flag {
                        Common => self.progress_pc(),
                        ExeC => self.c_progress_pc(),
                        Jump => {}
                    }
                }
            }
        }
    }

    // C拡張が有効かどうかを確認する関数
    pub fn is_c_extension_enabled(&self) -> bool {
        (self.read_raw_csr(CSR_MISA).unwrap() & 0x4) != 0
    }

    pub fn exception_direct_jump(&mut self) {
        let mtvec = self.read_raw_csr(CSR_MTVEC).unwrap();

        self.write_reg(Register::Pc, mtvec & !0x3).unwrap();
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
