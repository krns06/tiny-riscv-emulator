use std::{error::Error, path::Path};

use crate::{
    csr::{CSR, CSR_MEPC},
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

// エミュレータがexecしたときにその命令が何であるかを伝える列挙体
// jump系の命令だと命令後にpc+4をしなくて良くなるのでそれを伝えたりする。←これ以外の用途があるかはわからない。
enum EmulatorFlag {
    Jump,
    Common,
    TestEcall,
}

#[derive(Default)]
pub struct Emulator {
    memory: Memory<MEMORY_SIZE>,
    regs: [u64; 31],
    csr: CSR,
    pc: u64,
    current_priv: Priv,
    instruction: u32,
}

impl Emulator {
    // プログラムをロードする関数
    // 将来的にはロードする位置を指定できるようにしたい。
    // 遅延ロードとかもやってみたい。
    pub fn load<P: AsRef<Path>>(
        &mut self,
        filename: P,
    ) -> core::result::Result<(), Box<dyn Error>> {
        self.memory.load(filename)?;
        self.pc = 0;

        Ok(())
    }

    // メモリを読み込むときに使用する関数
    fn read_memory<const SIZE: usize>(&self, address: usize) -> Result<[u8; SIZE]> {
        Ok(self.memory.read::<SIZE>(address))
    }

    // メモリを書き込むときに使用する関数
    fn write_memory(&mut self, address: usize, values: &[u8]) -> Result<()> {
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

    // CSRを書き込む関数
    // 返り値はzero-extendされた値
    fn read_csr(&self, csr: u64) -> Result<u64> {
        self.csr.read_csr(csr)
    }

    // CSRを書き込む関数
    fn write_csr(&mut self, csr: u64, value: u64) -> Result<()> {
        self.csr.write_csr(csr, value)
    }

    fn check_misaligned(&self, offset: u64) -> Result<()> {
        if (self.pc.wrapping_add(offset)) % 4 == 0 {
            Ok(())
        } else {
            Err(InstructionAddressMissaligned)
        }
    }

    // 命令を取り出す関数
    // 暗黙的なメモリ読み込みはいくつか定義されているものを除き、例外は発生しない。
    // 現在読んでいる仕様書の部分ではfetchに相当する部分について特に明記されていないので現在のところは例外が起こらない想定とする。
    // run以外から呼んではいけない。
    fn fetch(&mut self) {
        self.instruction = u32::from_le_bytes(self.memory.read::<4>(self.pc as usize));
    }

    // 命令を格納するバイト列から実行する命令を判定し命令を実行する関数
    // 例外が発生した場合は即座にErrに起こった例外に対応するException型の値を返す。
    fn exec(&mut self) -> Result<EmulatorFlag> {
        // instruction == 0の場合は不正な命令である。
        // instruction & 0x3 != 3以外ならRV32もしくはRV64ではない可能性がある。
        if self.instruction == 0 || self.instruction & 0x3 != 3 {
            return Err(IllegralInstruction);
        }

        let op = (self.instruction >> 2) & 0x1f;
        let funct3 = (self.instruction >> 12) & 0x7;

        match op {
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

                match (funct3, imm >> 26) {
                    (0b000, _) => self.write_reg(
                        Register::X(rd),
                        self.read_reg(Register::X(rs1))?
                            .wrapping_add(sign_extend(11, imm)),
                    )?, //ADDI
                    (0b001, 0b000000) => {
                        // RV32IとRV64Iにどちらにも存在しているのでRV64Iの方を優先した。
                        self.write_reg(
                            Register::X(rd),
                            self.read_reg(Register::X(rs1))? << (imm & 0x3f),
                        )?;
                    } // SLLI
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
                    self.read_reg(Register::Pc)? + sign_extend(31, imm),
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
                    (0, 0b0100000) => self.write_reg(
                        Register::X(rd),
                        self.read_reg(Register::X(rs1))?
                            .wrapping_sub(self.read_reg(Register::X(rs2))?),
                    )?, // SUB
                    (0b111, 0) => self.write_reg(
                        Register::X(rd),
                        self.read_reg(Register::X(rs1))? & self.read_reg(Register::X(rs2))?,
                    )?,
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
                    (0, 0) => {
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
                    0b101 => {
                        flag = self.read_reg(Register::X(rs1))? as i64
                            >= self.read_reg(Register::X(rs2))? as i64;
                    } // BGE
                    0b111 => {
                        flag =
                            self.read_reg(Register::X(rs1))? >= self.read_reg(Register::X(rs2))?;
                    } // BGEU
                    _ => return Err(IllegralInstruction),
                };

                if flag {
                    self.check_misaligned(offset)?;

                    self.write_reg(
                        Register::Pc,
                        self.read_reg(Register::Pc)?.wrapping_add(offset),
                    )?;
                    return Ok(EmulatorFlag::Jump);
                }
            }
            0b11011 => {
                let (rd, imm) = extract_j_type(self.instruction);
                let offset = sign_extend(20, imm);

                let pc = self.read_reg(Register::Pc)?;

                // ターゲットアドレスが4byteでアライメントされていない場合はアライメントの例外を起こす。

                self.check_misaligned(offset)?;

                self.write_reg(Register::X(rd), pc + 4)?;
                self.write_reg(Register::Pc, pc + offset)?;
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
                            0x00000073 => {
                                let op = self.instruction & 0x7f;
                                let funct3 = (self.instruction >> 12) & 0x7;
                                eprintln!(
                                    "instruction: 0x{:08x} op: 0b{:07b} funct3: 0b{:03b}",
                                    self.instruction, op, funct3
                                );

                                eprintln!("[warning]: ecall may not work properly.");

                                // ecallをまともに実装していないがriscv-testsを行いため
                                // ecallが起こったときにTestEcallを返してgpの値を確認するようにする。

                                return Ok(EmulatorFlag::TestEcall);
                            } //ECALL
                            0x30200073 => match self.current_priv {
                                Priv::M => {
                                    // 全くちゃんと実装していないので今度実装する。
                                    // mstatusとか例外の扱いをちゃんと理解しないと実装しないほうが良さそう。
                                    eprintln!("[warning]: mret may not work properly.");

                                    // 多分MEPCは4byteアライメントされているからアライメントの例外は考えなくていい？
                                    self.write_reg(Register::Pc, self.read_csr(CSR_MEPC)?)?;
                                    return Ok(EmulatorFlag::Jump);
                                }
                                _ => {
                                    // Mモード以外で呼び出された場合は実装していない。
                                    unimplemented!("mret need to be invoked in M mode.");
                                }
                            }, // mret
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

                        self.write_reg(Register::X(rd), csr)?;

                        if rs1 != 0 {
                            self.write_csr(csr, csr ^ self.read_reg(Register::X(rs1))?)?;
                        }
                    } // CSRRS
                    0b101 => {
                        let csr = if rd != 0 { self.read_csr(imm)? } else { 0 };
                        self.write_csr(imm, rs1 as u64)?;

                        if rd != 0 {
                            self.write_reg(Register::X(rd), csr)?;
                        }
                    } // CSRRWI
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

    pub fn run(&mut self) {
        loop {
            eprintln!("PC: 0x{:016x}", self.pc,);
            self.fetch();

            match self.exec() {
                Err(e) => {
                    match self.current_priv {
                        Priv::M => {
                            // MEPCへ例外がおこったアドレスを書き込む
                            self.write_csr(CSR_MEPC, self.pc).unwrap();
                        }
                        _ => unimplemented!("The mechanism for writing the address where the exception occurred to the *EPC is not implemented."),
                    }

                    let op = self.instruction & 0x7f;
                    let funct3 = (self.instruction >> 12) & 0x7;
                    panic!(
                        "instruction: 0x{:08x} op: 0b{:07b} funct3: 0b{:03b}\nException: {:?}",
                        self.instruction, op, funct3, e
                    );
                }
                Ok(flag) => {
                    use EmulatorFlag::*;

                    match flag {
                        Common => self.progress_pc(),
                        Jump => {}
                        TestEcall => {
                            eprintln!("[info]: TestEcall occurred");
                            break;
                        }
                    }
                }
            }
        }
    }

    pub fn regs(&self) -> &[u64] {
        &self.regs
    }

    pub fn pc(&self) -> u64 {
        self.pc
    }

    pub fn memory(&self) -> &Memory<MEMORY_SIZE> {
        &self.memory
    }

    pub fn show_regs(&self) {
        eprintln!("---------- REGS ----------");
        eprintln!("x00: 0x{:016x}", 0);

        for (i, reg) in self.regs.iter().enumerate() {
            eprintln!("x{:02}: 0x{:016x}", i + 1, reg);
        }
        eprintln!("---------- REGS ----------");
    }
}
