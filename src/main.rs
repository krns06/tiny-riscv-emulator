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

    emulator.set_c_extenstion(true);
    let um_tests = ["rv64mi-p-csr.bin"];

    let name = "um_tests";

    display_start_test(name);
    for test in um_tests {
        run_test(&mut emulator, test, 0x1000);
    }
    display_end_test(name);
}
