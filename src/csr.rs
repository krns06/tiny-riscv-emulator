use crate::{
    emulator::Emulator,
    exception::Exception::{self, *},
    Priv, Result,
};

pub(crate) const CSR_SSTATUS: u64 = 0x100;
pub(crate) const CSR_SEPC: u64 = 0x141;
pub(crate) const CSR_MSTATUS: u64 = 0x300;
pub(crate) const CSR_MISA: u64 = 0x301;
pub(crate) const CSR_MEDELEG: u64 = 0x302;
pub(crate) const CSR_MIDELEG: u64 = 0x303;
pub(crate) const CSR_MIE: u64 = 0x304;
pub(crate) const CSR_MTVEC: u64 = 0x305;
const CSR_MCOUNTEREN: u64 = 0x306;
pub(crate) const CSR_MEPC: u64 = 0x341;
pub(crate) const CSR_MIP: u64 = 0x344;
pub(crate) const CSR_MCAUSE: u64 = 0x342;
pub(crate) const CSR_MTVAL: u64 = 0x343;

const CSR_CYCLE: u64 = 0xc00;

pub(crate) const CSR_MSTATUS_MPP_MASK: u64 = 3 << 11;
pub(crate) const CSR_MSTATUS_SPP_MASK: u64 = 1 << 8;
pub(crate) const CSR_MSTATUS_MIE_MASK: u64 = 1 << 3;
pub(crate) const CSR_MSTATUS_SIE_MASK: u64 = 1 << 1;
pub(crate) const CSR_MSTATUS_MPIE_MASK: u64 = 1 << 7;
pub(crate) const CSR_MSTATUS_SPIE_MASK: u64 = 1 << 5;
pub(crate) const CSR_MSTATUS_TVM_MASK: u64 = 1 << 20;
pub(crate) const CSR_MSTATUS_TSR_MASK: u64 = 1 << 22;
pub(crate) const CSR_MSTATUS_TW_MASK: u64 = 1 << 21;
pub(crate) const CSR_MSTATUS_MPRV_MASK: u64 = 1 << 17;
const CSR_MSTATUS_XXL_MASK: u64 = 0xa << 32;

// 現在実装しているxstatus系のマスク
const CSR_MSTATUS_MASK: u64 = 0x4019aa;
pub(crate) const CSR_SSTATUS_MASK: u64 = 0x122;

// si{e,p}についてサポートするマスク
const CSR_SIX_MASK: u64 = 0x222;

const CAUSE_INTERRUPT_MASK: u64 = 0x2aaa;
const CAUSE_EXCEPTION_MASK: u64 = 0xcbbff;

#[derive(Debug)]
pub(crate) struct Csr {
    stvec: u64,      // 0x105
    scounteren: u64, // 0x106

    sepc: u64, // 0x141
    satp: u64, // 0x180

    mstatus: u64, // 0x300 or 0x100(sstatus)
    misa: u64,    // 0x301
    mtvec: u64,   // 0x305

    medeleg: u64,    // 0x302
    mideleg: u64,    // 0x303
    mie: u64,        // 0x304
    mcounteren: u64, // 0x306
    mscratch: u64,   // 0x340
    mepc: u64,       // 0x341
    mcause: u64,     // 0x342
    mtval: u64,      // 0x343
    mip: u64,        // 0x344
    pmpcfg0: u64,    // 0x3a0
    pmpaddr0: u64,   // 0x3b0

    mnstatus: u64, // 0x744

    mcycle: u64, // 0x800
}

impl Default for Csr {
    fn default() -> Self {
        Self {
            stvec: 0,
            scounteren: 0,
            sepc: 0,
            satp: 0,
            mstatus: CSR_MSTATUS_XXL_MASK,
            misa: (1 << 63) | 0x141105, // (64bit,imacsu)
            mtvec: 0,
            medeleg: 0,
            mideleg: 0,
            mie: 0,
            mcounteren: 0,
            mscratch: 0,
            mepc: 0,
            mcause: 0,
            mtval: 0,
            mip: 0,
            pmpcfg0: 0,
            pmpaddr0: 0,
            mnstatus: 0,
            mcycle: 0,
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

    // 割り込みがアクティブかどうかを判定しアクティブな場合はErrとして割り込み用のExceptionを返す
    // サポートされていない条件の場合かつ仕様書では割り込みの条件に入っている場合も実装していない場合はOKを返すので注意
    pub(crate) fn check_interrupt_active(&self) -> Result<()> {
        let mstatus = self.read_raw_csr(CSR_MSTATUS).unwrap();

        // mstatus.MIEが有効でない場合(0)は返す
        if mstatus & CSR_MSTATUS_MIE_MASK == 0 {
            return Ok(());
        }

        // mstatus.MIEが有効な場合

        if self.current_priv != Priv::M {
            return Ok(());
        }

        let mie = self.read_raw_csr(CSR_MIE).unwrap();
        let mip = self.read_raw_csr(CSR_MIP).unwrap();

        let active = mie & mip;

        if active != 0 {
            if active.count_ones() != 1 {
                panic!("Error: Nested traps are not supported.");
            }

            match active {
                2 => return Err(Exception::SuperSoftInt),
                _ => panic!("Error: The active interrupt is not suported."),
            }
        }

        Ok(())
    }

    // 命令がCPUで実行されたときにサイクルを１つ増やす
    // mcountinhibitが実装され場合はここのサイクルを制御できるようにする。
    pub(crate) fn add_cycle(&mut self) {
        self.csr.mcycle += 1;
    }

    // 暗黙的にcsrを読み込む関数
    // 権限やRWのチェック等を終わった段階で呼ぶ関数
    // エイリアス等が存在するCSRを読み込む場合に対応するための関数
    // 副作用はなく、ただ単純にCSRをよむのみを行う。
    // そのCSRが存在しない場合はIllegralInstructionを返す。
    pub(crate) fn read_raw_csr(&self, csr: u64) -> Result<u64> {
        match csr {
            CSR_SSTATUS => Ok(self.csr.mstatus & CSR_SSTATUS_MASK), // sstatus
            CSR_SEPC => Ok(self.csr.sepc),                          // sepc
            0x180 => Ok(self.csr.satp),                             // satp
            CSR_MSTATUS => Ok(self.csr.mstatus),                    // mstatus
            CSR_MISA => Ok(self.csr.misa),                          // misa
            CSR_MEDELEG => Ok(self.csr.medeleg),                    // medeleg
            CSR_MIDELEG => Ok(self.csr.mideleg),                    // mideleg
            CSR_MIE => Ok(self.csr.mie),                            // mie
            CSR_MTVEC => Ok(self.csr.mtvec),                        // mtvec
            CSR_MCOUNTEREN => Ok(self.csr.mcounteren),              // mcounteren
            0x340 => Ok(self.csr.mscratch),                         // mscratch
            CSR_MEPC => Ok(self.csr.mepc),                          // mepc
            CSR_MCAUSE => Ok(self.csr.mcause),                      // mcause
            CSR_MTVAL => Ok(self.csr.mtval),                        // mtval
            CSR_MIP => Ok(self.csr.mip),                            // mip
            0x800 | CSR_CYCLE => Ok(self.csr.mcycle),               // mcycle or cycle
            0xf11 => Ok(0xba5eba11),                                // mvendorid(baseball)
            0xf12 => Ok(0x05500550),                                // mvendorid(ossoosso)
            0xf13 => Ok(0x1),                                       // mimpid(version 1)
            0xf14 => Ok(0),                                         // mhartid
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

        match csr {
            CSR_CYCLE => {
                if self.current_priv != Priv::M
                    && (self.read_raw_csr(CSR_MCOUNTEREN).unwrap() & 0x1) == 0
                {
                    return Err(IllegralInstruction);
                }

                self.read_raw_csr(csr)
            } // cycle
            _ => self.read_raw_csr(csr),
        }
    }

    // CSRを書き込む関数
    pub(crate) fn write_csr(&mut self, csr: u64, value: u64) -> Result<()> {
        if csr >> 12 != 0 {
            panic!("Error: Unknown csr 0x{:016x}", csr);
        }

        if (csr >> 10) & 0x3 == 0b11 {
            // read only
            return Err(IllegralInstruction);
        }

        self.check_csr_priv(csr)?;

        eprintln!("[info]: write 0x{:x}[csr] value: 0x{:x}", csr, value);

        match csr {
            CSR_SSTATUS => {
                if value & 0x80_00_00_01_00_01_e6_40 != 0 {
                    // 下の条件を満たす場合は一旦エラーを出すようにする。
                    // * UBEがbig endian(1)
                    // * xS, SDが１
                    // * UXLが64bit以外(01, 11)
                    eprintln!(
                        "[warning]: The value(0x{:016x}) of writing sstatus is not support.",
                        value
                    );
                    return Err(IllegralInstruction);
                }

                if value & 0x3 << 17 != 0 {
                    // MXRとSUMを実装したらこの警告を消す
                    // 現状はCSR_xSTATUS_MASKによってMXRとSUMはクリアされる
                    eprintln!("[warning]: sstatus.MXR or sstatus.SUM is not supported.");
                }

                self.csr.mstatus =
                    (self.csr.mstatus & !CSR_SSTATUS_MASK) | (value & CSR_SSTATUS_MASK);
            } // sstatus
            0x105 => {
                // mtvecと同様
                self.csr.stvec = value & 0xfffffffffffffffd;
            } // stvec
            0x106 => {
                self.csr.scounteren = value;
            } // scounteren
            CSR_SEPC => {
                // とりあえず4byteのアライメントにする
                self.csr.sepc = value & 0xfffffffffffffffc;
            } // sepc
            0x180 => {
                // Bareモードのみサポート
                // Sモードをまともに実装するまでは何も行わないことにする。

                if value != 0 {
                    return Err(IllegralInstruction);
                }

                eprint_not_working("satp");
            } // satp
            CSR_MSTATUS => {
                if value & 0x8000_0005_002f_e640 != 0 {
                    // 下の条件を満たす場合は一旦エラーを出すようにする。
                    // * xBEがbig endian(1)
                    // * VSやFS、XSに対して書き込みがある場合
                    // * SDへの書き込み
                    // * MXRが1
                    // * ハイパバイザー関連のパラメータ
                    // * xXLが64bit以外(01, 11)
                    eprintln!(
                        "[warning]: The value(0x{:016x}) of writing mstatus is not support.",
                        value
                    );
                    return Err(IllegralInstruction);
                }

                // Mモードでの書き込みの想定なので制限は特にない。
                // self.csr.mstatus = 0xa00000000 & (value & 0x8000_003f_007f_ffea);
                self.csr.mstatus = (value & CSR_MSTATUS_MASK) | CSR_MSTATUS_XXL_MASK;
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
                self.csr.mtvec = value & 0xfffffffffffffffd;
            } // mtvec
            0x302 => {
                // カスタム用途は一旦は無視する。
                self.csr.medeleg = value & CAUSE_EXCEPTION_MASK;
            } // medeleg
            0x303 => {
                self.csr.mideleg = value & CAUSE_INTERRUPT_MASK;
            } // mideleg
            CSR_MIE => {
                // LCOFIPはサポートしない
                self.csr.mie = value & 0xaaa;
            } // mie
            CSR_MCOUNTEREN => {
                self.csr.mcounteren = value;
            } // mcounteren
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
                //
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
            CSR_MIP => {
                // このレジスタは割り込みが起こっているかを示すレジスタらしい
                self.csr.mip = value & 0xaaa;
            } // mip
            0x3a0 => {
                self.csr.pmpcfg0 = value;
                eprint_not_working("pmpcfg0");
            } // pmpcfg0
            0x3b0 => {
                self.csr.pmpaddr0 = value & 0x3ffffffffffff;
                eprint_not_working("pmpaddr0");
            } // pmpaddr0
            0x744 => {
                self.csr.mnstatus = value & 0x8;
                eprint_not_working("mnstatus");
            } // mnstatus
            0xf14 => {} // mhartid
            _ => return Err(IllegralInstruction),
        }

        Ok(())
    }
}

fn eprint_not_working(name: &str) {
    eprintln!("[warning]: {} may not work properly.", name);
}
