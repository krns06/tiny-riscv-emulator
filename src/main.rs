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
        "rv64mi-p-csr.bin",
        "rv64mi-p-illegal.bin",
        "rv64mi-p-ld-misaligned.bin",
        "rv64mi-p-lh-misaligned.bin",
        "rv64mi-p-lw-misaligned.bin",
        "rv64mi-p-ma_addr.bin",
        "rv64mi-p-ma_fetch.bin",
        "rv64mi-p-mcsr.bin",
        "rv64mi-p-sd-misaligned.bin",
        "rv64mi-p-sh-misaligned.bin",
        "rv64mi-p-sw-misaligned.bin",
        "rv64mi-p-scall.bin",
    ];

    display_start_test(name);
    for test in si_tests {
        run_test(&mut emulator, test, 0x1000);
    }
    display_end_test(name);
}
