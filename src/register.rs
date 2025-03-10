// レジスターを表す列挙体
// Xの値は0~31以外はパニックになる。
#[derive(Debug)]
pub enum Register {
    X(u8),
    Pc,
}
