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
        "rv64ua-p-amoadd_d.bin",
        "rv64ua-p-amoadd_w.bin",
        "rv64ua-p-amoand_d.bin",
        "rv64ua-p-amoand_w.bin",
        "rv64ua-p-amomax_d.bin",
        "rv64ua-p-amomax_w.bin",
        "rv64ua-p-amomaxu_d.bin",
        "rv64ua-p-amomaxu_w.bin",
        "rv64ua-p-amomin_d.bin",
        "rv64ua-p-amomin_w.bin",
        "rv64ua-p-amominu_d.bin",
        "rv64ua-p-amominu_w.bin",
        "rv64ua-p-amoor_d.bin",
        "rv64ua-p-amoor_w.bin",
        "rv64ua-p-amoswap_d.bin",
        "rv64ua-p-amoswap_w.bin",
        "rv64ua-p-amoxor_d.bin",
        "rv64ua-p-amoxor_w.bin",
        "rv64ua-p-lrsc.bin",
    ];

    display_start_test(name);
    for test in si_tests {
        run_test(&mut emulator, test, 0x1000);
    }
    display_end_test(name);
}
