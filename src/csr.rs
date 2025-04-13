use crate::{exception::Exception::*, Priv, Result};

pub const CSR_MEPC: u64 = 0x341;

#[derive(Default)]
pub struct CSR {
    current_priv: Priv, // 現在の権限

    mtvec: u64, // 0x305

    // mstatus: u64, // 0x300
    // medeleg: u64, // 0x302
    // mideleg: u64, // 0x302
    // mie: u64, // 0x304
    mepc: u64,     // 0x341
    pmpcfg0: u64,  // 0x3a0
    pmpaddr0: u64, // 0x3b0

    mnstatus: u64, // 0x744
}

impl CSR {
    pub fn set_priv(&mut self, p: Priv) {
        self.current_priv = p;
    }

    pub fn initialize_csr(&mut self) {
        *self = CSR::default();
    }

    // CSRを書き込む関数
    // 返り値はzero-extendされた値
    pub fn read_csr(&self, csr: u64) -> Result<u64> {
        if csr >> 12 != 0 {
            panic!("Error: Unknown csr 0x{:016x}", csr);
        }

        if (csr >> 8) & 0x3 > self.current_priv as u64 {
            return Err(IllegralInstruction);
        }

        eprintln!("[info]: read 0x{:x}[csr]", csr);

        match csr {
            0x305 => Ok(self.mtvec), //  mtvec
            0x341 => Ok(self.mepc),  // mepc
            0xf14 => Ok(0),          // mhartid
            _ => Err(IllegralInstruction),
        }
    }

    // CSRを書き込む関数
    pub fn write_csr(&mut self, csr: u64, value: u64) -> Result<()> {
        if csr >> 12 != 0 {
            panic!("Error: Unknown csr 0x{:016x}", csr);
        }

        if (csr >> 10) & 0x3 == 0b11 {
            return Err(IllegralInstruction);
        }

        if (csr >> 8) & 0x3 > self.current_priv as u64 {
            return Err(IllegralInstruction);
        }

        eprintln!("[info]: write 0x{:x}[csr] value: 0x{:x}", csr, value);

        match csr {
            0x180 => {
                // Bareモードのみサポート
                // Sモードをまともに実装するまでは何も行わないことにする。

                eprint_not_work("satp");
            } // satp
            0x300 => {
                if value != 0 {
                    unimplemented!("mstatus is not implmented");
                }
                eprint_not_work("mstatus");
            } // mstatus
            0x305 => {
                // 現在はMODEは
                // 0: Direct
                // 1: Vectored
                // のみしかサポートされていない。
                // WARL
                self.mtvec = value & 0xfffffffd;
            } // mtvec
            0x302 => {
                if value != 0 {
                    // 割り込みの場合は実装していない
                    unimplemented!("medeleg is not implmented.")
                }
                eprint_not_work("medeleg");
            } // medeleg
            0x303 => {
                if value != 0 {
                    // 割り込みの場合は実装していない
                    unimplemented!("mideleg is not implmented.")
                }
                eprint_not_work("mideleg");
            } // mideleg
            0x304 => {
                if value != 0 {
                    unimplemented!("mie is not implmented.");
                }

                eprint_not_work("mie");
            } // mie
            0x341 => {
                // とりあえず4byteのアライメントにする
                self.mepc = value & 0xfffffffffffffffc;
            } // mepc
            0x3a0 => {
                self.pmpcfg0 = value;
                eprint_not_work("pmpcfg0");
            } // pmpcfg0
            0x3b0 => {
                self.pmpaddr0 = value & 0x3ffffffffffff;
                eprint_not_work("pmpaddr0");
            } // pmpaddr0
            0x744 => {
                self.mnstatus = value & 0x8;
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
