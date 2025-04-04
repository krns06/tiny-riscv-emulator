use tiny_riscv_emulator::emulator::Emulator;

fn main() {
    let mut emulator = Emulator::default();

    // メモリをなぜかスタックに固定で確保する実装になっているのでそれを
    // ヒープにするまで$cargo rでテストを行うことにする。

    const TEST_DIR: &str = "tests/isa/flats";
    const TESTS: [&str; 4] = [
        "rv64ui-p-add.bin",
        "rv64ui-p-addi.bin",
        "rv64ui-p-addiw.bin",
        "rv64ui-p-addw.bin",
    ];

    for test in TESTS {
        emulator.load(format!("{}/{}", TEST_DIR, test)).unwrap();

        emulator.run();

        // gp(3)が1であることを確認する。
        assert!(emulator.regs()[2] == 1);
        println!("[info]: Test {} was successful.", test);
    }
}
