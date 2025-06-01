pub mod cpu;
pub mod csr;
pub mod emulator;
pub mod exception;
pub mod memory;
pub mod register;

pub type Result<T> = std::result::Result<T, crate::exception::Exception>;

// 権限を示す列挙体
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Priv {
    U = 0,
    S = 1,
    M = 3,
}

impl From<u64> for Priv {
    fn from(value: u64) -> Self {
        match value {
            0 => Priv::U,
            1 => Priv::S,
            3 => Priv::M,
            _ => panic!("Error: Failed to convert from {} to Priv.", value),
        }
    }
}

impl Default for Priv {
    fn default() -> Self {
        Self::M
    }
}
