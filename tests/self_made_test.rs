use tiny_riscv_emulator::emulator::Emulator;

const TEST_DIR: &str = "tests/self_made_test_src";

fn run_and_assert(filename: &str, ans: &[u64; 32]) {
    let mut emulator = Emulator::default();

    emulator.load(format!("{}/{}", TEST_DIR, filename)).unwrap();
    emulator.run();

    assert!(&ans[1..] == emulator.regs());
}

#[test]
fn return_100() {
    let ans = [
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 512, 257, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0,
    ];

    run_and_assert("return_100.bin", &ans);
}
