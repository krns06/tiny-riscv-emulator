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
        "rv64um-p-div.bin",
        "rv64um-p-divu.bin",
        "rv64um-p-divuw.bin",
        "rv64um-p-divw.bin",
        "rv64um-p-mul.bin",
        "rv64um-p-mulh.bin",
        "rv64um-p-mulhsu.bin",
        "rv64um-p-mulhu.bin",
        "rv64um-p-mulw.bin",
        "rv64um-p-rem.bin",
        "rv64um-p-remu.bin",
        "rv64um-p-remuw.bin",
        "rv64um-p-remw.bin",
    ];

    display_start_test(name);
    for test in si_tests {
        run_test(&mut emulator, test, 0x1000);
    }
    display_end_test(name);
}
