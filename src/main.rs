use tiny_riscv_emulator::emulator::Emulator;
const TEST_DIR: &str = "tests/isa/flats";

fn display_start_test(name: &str) {
    eprintln!("[info]: start {}", name);
}

fn display_end_test(name: &str) {
    eprintln!("[info]: end {}", name);
}

fn run_test(emulator: &mut Emulator, test: &str, riscv_tests_exit_memory_address: usize) {
    emulator.load(format!("{}/{}", TEST_DIR, test)).unwrap();
    emulator.set_riscv_tests_exit_memory_address(riscv_tests_exit_memory_address);

    emulator.run();

    if emulator.check_riscv_tests_result() {
        println!("[info]: Test {} was successful.", test);
    } else {
        panic!("[Error]: {} was failed.", test);
    }
}

fn main() {
    let mut emulator = Emulator::default();

    let name = "si_tests";

    let si_tests = [
        "rv64si-p-csr.bin",
        "rv64si-p-ma_fetch.bin",
        "rv64si-p-scall.bin",
        "rv64si-p-wfi.bin",
    ];

    display_start_test(name);
    for test in si_tests {
        run_test(&mut emulator, test, 0x1000);
    }
    display_end_test(name);
}
