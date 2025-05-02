use crate::{emulator::Emulator, exception::Exception::*, Result};

pub(crate) const CSR_MSTATUS: u64 = 0x300;
pub(crate) const CSR_MISA: u64 = 0x301;
pub(crate) const CSR_MEDELEG: u64 = 0x302;
pub(crate) const CSR_MIDELEG: u64 = 0x303;
pub(crate) const CSR_MTVEC: u64 = 0x305;
pub(crate) const CSR_MEPC: u64 = 0x341;
pub(crate) const CSR_MCAUSE: u64 = 0x342;
pub(crate) const CSR_MTVAL: u64 = 0x343;

pub(crate) const CSR_MSTATUS_MPP_MASK: u64 = 3 << 11;
pub(crate) const CSR_MSTATUS_MIE_MASK: u64 = 1 << 3;
pub(crate) const CSR_MSTATUS_MPIE_MASK: u64 = 1 << 7;

const CAUSE_INTERRUPT_MASK: u64 = 0x2aaa;
const CAUSE_EXCEPTION_MASK: u64 = 0xcbbff;

#[derive(Debug)]
pub(crate) struct Csr {
    mstatus: u64, // 0x300
    misa: u64,    // 0x301
    mtvec: u64,   // 0x305

    medeleg: u64,  // 0x302
    mideleg: u64,  // 0x303
    mie: u64,      // 0x304
    mscratch: u64, // 0x340
    mepc: u64,     // 0x341
    mcause: u64,   // 0x342
    mtval: u64,    // 0x343
    mip: u64,      // 0x344
    pmpcfg0: u64,  // 0x3a0
    pmpaddr0: u64, // 0x3b0

    mnstatus: u64, // 0x744
}

impl Default for Csr {
    fn default() -> Self {
        Self {
            mstatus: 0xa00000000,
            misa: (1 << 63) | 0x1105, // (64bit,ima)
            mtvec: 0,
            medeleg: 0,
            mideleg: 0,
            mie: 0,
            mscratch: 0,
            mepc: 0,
            mcause: 0,
            mtval: 0,
            mip: 0,
            pmpcfg0: 0,
            pmpaddr0: 0,
            mnstatus: 0,
        }
    }
}

impl Emulator {
    pub(crate) fn initialize_csr(&mut self) {
        self.csr = Csr::default();
    }

    // C拡張を有効/無効にする関数
    // 有効にする場合は制限はないが、無効にする場合はプログラムの命令から呼ばれる想定
    fn set_c_extenstion(&mut self, enabled: bool) {
        if enabled {
            self.csr.misa |= 4;
        } else {
            // 無効にする場合は次の命令がIALIGN(このエミュレータだと32)になっていない場合は変更しないらしい。
            // 例外は起こらないっぽい
            // 無効になっている場合に呼ばれる場合は考慮する必要がないので有効になっている場合に、次の命令がIALIGNになっているか確認し、なっていない場合は無効にしない。

            if self
                .check_misaligned_nbyte_misaligned(self.pc + 2, 4)
                .is_ok()
            {
                self.csr.misa &= !4;
            }
        }
    }

    fn check_csr_priv(&self, csr: u64) -> Result<()> {
        if (csr >> 8) & 0x3 > self.current_priv as u64 {
            Err(IllegralInstruction)
        } else {
            Ok(())
        }
    }

    // 暗黙的にcsrを読み込む関数
    // 権限やRWのチェック等を終わった段階で呼ぶ関数
    // エイリアス等が存在するCSRを読み込む場合に対応するための関数
    // 副作用はなく、ただ単純にCSRをよむのみを行う。
    // そのCSRが存在しない場合はIllegralInstructionを返す。
    pub(crate) fn read_raw_csr(&self, csr: u64) -> Result<u64> {
        match csr {
            CSR_MSTATUS => Ok(self.csr.mstatus), // mstatus
            CSR_MISA => Ok(self.csr.misa),       // misa
            CSR_MEDELEG => Ok(self.csr.medeleg), // medeleg
            CSR_MIDELEG => Ok(self.csr.mideleg), // mideleg
            CSR_MTVEC => Ok(self.csr.mtvec),     // mtvec
            0x340 => Ok(self.csr.mscratch),      // mscratch
            CSR_MEPC => Ok(self.csr.mepc),       // mepc
            CSR_MCAUSE => Ok(self.csr.mcause),   // mcause
            CSR_MTVAL => Ok(self.csr.mtval),     // mtval
            0xf11 => Ok(0xba5eba11),             // mvendorid(baseball)
            0xf12 => Ok(0x05500550),             // mvendorid(ossoosso)
            0xf13 => Ok(0x1),                    // mimpid(version 1)
            0xf14 => Ok(0),                      // mhartid
            _ => Err(IllegralInstruction),
        }
    }

    // CSRを読み込む関数
    pub(crate) fn read_csr(&self, csr: u64) -> Result<u64> {
        if csr >> 12 != 0 {
            panic!("Error: Unknown csr 0x{:016x}", csr);
        }

        self.check_csr_priv(csr)?;

        eprintln!("[info]: read 0x{:x}[csr]", csr);

        self.read_raw_csr(csr)
    }

    // CSRを書き込む関数
    pub(crate) fn write_csr(&mut self, csr: u64, value: u64) -> Result<()> {
        if csr >> 12 != 0 {
            panic!("Error: Unknown csr 0x{:016x}", csr);
        }

        if (csr >> 10) & 0x3 == 0b11 {
            return Err(IllegralInstruction);
        }

        self.check_csr_priv(csr)?;

        eprintln!("[info]: write 0x{:x}[csr] value: 0x{:x}", csr, value);

        match csr {
            0x180 => {
                // Bareモードのみサポート
                // Sモードをまともに実装するまでは何も行わないことにする。

                if value != 0 {
                    return Err(IllegralInstruction);
                }

                eprint_not_work("satp");
            } // satp
            CSR_MSTATUS => {
                if value & 0x8000_000a_007f_e640 != 0 {
                    // 下の条件を満たす場合は一旦エラーを出すようにする。
                    // * xBEがbig endian(1)
                    // * VSやFS、XSに対して書き込みがある場合
                    // * SDへの書き込み
                    // * ハイパバイザー関連のパラメータ
                    // * xXLが64bit以外(00, 11)
                    eprintln!(
                        "[warning]: The value(0b{:b}) of writing mstatus is not support.",
                        value
                    );
                    return Err(IllegralInstruction);
                }

                // Mモードでの書き込みの想定なので制限は特にない。
                // self.csr.mstatus = 0xa00000000 & (value & 0x8000_003f_007f_ffea);
                self.csr.mstatus = value & 0x35000019aa;
            } // mstatus
            0x301 => {
                // C拡張を無効/有効にする以外を想定しない。
                self.set_c_extenstion((value >> 2) & 0x1 == 1);
            } // misa
            CSR_MTVEC => {
                // 現在はMODEは
                // 0: Direct
                // 1: Vectored
                // のみしかサポートされていない。
                // WARL
                self.csr.mtvec = value & 0xfffffffd;
            } // mtvec
            0x302 => {
                // カスタム用途は一旦は無視する。
                self.csr.medeleg = value & CAUSE_EXCEPTION_MASK;
            } // medeleg
            0x303 => {
                self.csr.mideleg = value & CAUSE_INTERRUPT_MASK;
            } // mideleg
            0x304 => {
                // LCOFIPはサポートしない
                self.csr.mie = value & 0xaaa;
            } // mie
            0x340 => {
                self.csr.mscratch = value;
            } // mscratch
            0x341 => {
                // とりあえず4byteのアライメントにする
                self.csr.mepc = value & 0xfffffffffffffffc;
            } // mepc
            CSR_MCAUSE => {
                // ソフトウェアからの書き込みはしてはいけない。
                // その仕組みを実装していないのでCSR書き込む系の命令を実行された場合は変更されてしまう。
                self.csr.mcause = value
                    & if value >> 63 == 1 {
                        let value = value & !(1 << 63);

                        match value {
                            1 | 3 | 5 | 7 | 9 | 11 | 13 => value,
                            _ => 0,
                        }
                    } else {
                        match value {
                            0..=9 | 11..=13 | 15 | 18..=19 => value,
                            _ => 0,
                        }
                    };
            } // mcause
            CSR_MTVAL => {
                // IllegralInstructionのときとaccess-faultとpage-faultのときは仕様にしたがって値をいれる。それ以外のときは0。
                // 上のトラップのあとに別のトラップ時に0にする仕組みがないとバグりそう。
                self.csr.mtval = value;
            } // mtval
            0x344 => {
                // 仕様的にはMEIP等はプラットフォーム特有の割り込みコントローラなどでsetとclearしないといけないらしいが
                // このエミュレータでは普通にsetできるということにする。
                self.csr.mip = value & 0xaaa;
            } // mip
            0x3a0 => {
                self.csr.pmpcfg0 = value;
                eprint_not_work("pmpcfg0");
            } // pmpcfg0
            0x3b0 => {
                self.csr.pmpaddr0 = value & 0x3ffffffffffff;
                eprint_not_work("pmpaddr0");
            } // pmpaddr0
            0x744 => {
                self.csr.mnstatus = value & 0x8;
                eprint_not_work("mnstatus");
            } // mnstatus
            0xf14 => {} // mhartid
            _ => return Err(IllegralInstruction),
        }

        Ok(())
    }
}

fn eprint_not_work(name: &str) {
    eprintln!("[warning]: {} may not work properly.", name);
}
