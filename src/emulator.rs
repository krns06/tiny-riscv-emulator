use std::{error::Error, path::Path, result};

use crate::{exception::Exception, memory::Memory, register::Register};

// 現在は1M byte
const MEMORY_SIZE: usize = 1024;

type Result<T> = result::Result<T, Exception>;

fn extract_i_type(instruction: u32) -> (u8, u8, u64) {
    let rd = (instruction >> 7) & 0x1f;
    let rs1 = (instruction >> 15) & 0x1f;
    let imm = (instruction >> 20) as u64;

    (rd as u8, rs1 as u8, imm)
}

#[derive(Default)]
pub struct Emulator {
    memory: Memory<MEMORY_SIZE>,
    regs: [u64; 31],
    pc: u64,
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

    // メモリを書き込むときに使用する関数
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

    // 命令を取り出す関数
    // 暗黙的なメモリ読み込みはいくつか定義されているものを除き、例外は発生しない。
    // 現在読んでいる仕様書の部分ではfetchに相当する部分について特に明記されていないので現在のところは例外が起こらない想定とする。
    // run以外から呼んではいけない。
    fn fetch(&mut self) -> u32 {
        u32::from_le_bytes(self.memory.read::<4>(self.pc as usize))
    }

    // 命令を格納するバイト列から実行する命令を判定し命令を実行する関数
    // 例外が発生した場合は即座にErrに起こった例外に対応するException型の値を返す。
    fn exec(&mut self, instruction: u32) -> Result<()> {
        // instruction == 0の場合は不正な命令である。
        // instruction & 0x3 != 3以外ならRV32もしくはRV64ではない可能性がある。
        if instruction == 0 || instruction & 0x3 != 3 {
            return Err(Exception::IllegralInstruction(instruction));
        }

        let op = (instruction >> 2) & 0x1f;
        let func3 = (instruction >> 12) & 0x7;

        match op {
            0b00100 => {
                let (rd, rs1, imm) = extract_i_type(instruction);

                match func3 {
                    0 => self.write_reg(Register::X(rd), self.read_reg(Register::X(rs1))? + imm),
                    v => Err(Exception::IllegralInstruction(v)),
                }
            }
            v => {
                unimplemented!("Error: {:?} is not implemented.", v);
            }
        }
    }

    // pcを一つ(4byte)進ませる関数
    // run以外で呼ばれずに、例外を起こす可能性がないのでread_regを呼び出していない。
    // run以外から呼んではいけない。
    fn progress_pc(&mut self) {
        self.pc += 4;
    }

    pub fn run(&mut self) {
        loop {
            let instruction = self.fetch();

            match self.exec(instruction) {
                Err(e) => {
                    match e {
                        Exception::IllegralInstruction(0) => {
                            // とりあえず0の命令が現れた場合にプログラムを終了する。
                            return;
                        }
                        _ => {
                            // 例外が発生した場合の処理
                            // 現在は処理内容は決めていないのでpanicにする。
                            panic!("Exception: {:?}", e);
                        }
                    }
                }
                Ok(()) => {
                    self.progress_pc();
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
}
