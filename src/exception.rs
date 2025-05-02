#[derive(Debug, Clone, Copy)]
#[repr(u64)]
pub enum Exception {
    // branchかjump命令を実行したときにターゲットアドレスが4byte(or2byte)のアライメントになっていなかったら起こる。
    InstructionAddressMissaligned = 0,
    IllegralInstruction = 2,
    EnvironmentCallFromUMode = 8,
    EnvironmentCallFromSMode = 9,
    EnvironmentCallFromMMode = 11,
}
