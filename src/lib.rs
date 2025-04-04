pub mod csr;
pub mod emulator;
pub mod exception;
pub mod memory;
pub mod register;

pub type Result<T> = std::result::Result<T, crate::exception::Exception>;

// 権限を示す列挙体
#[derive(Clone, Copy)]
pub enum Priv {
    U = 0,
    S = 1,
    M = 3,
}

impl Default for Priv {
    fn default() -> Self {
        Self::M
    }
}
