#[derive(Debug)]
#[repr(u64)]
pub enum Exception {
    // branchかjump命令を実行したときにターゲットアドレスが4byte(or2byte)のアライメントになっていなかったら起こる。
    InstructionAddressMissaligned = 0,
    IllegralInstruction = 2,
}
