use tiny_riscv_emulator::emulator::Emulator;
const TEST_DIR: &str = "tests/isa/flats";

fn run_tests(emulator: &mut Emulator, name: &str, tests: &[&str]) {
    eprintln!("[info]: start {}", name);
    for test in tests {
        emulator.load(format!("{}/{}", TEST_DIR, test)).unwrap();

        emulator.run();

        if emulator.check_riscv_tests_result() {
            println!("[info]: Test {} was successful.", test);
        } else {
            panic!("[Error]: {} was failed.", test);
        }
    }
    eprintln!("[info]: end {}", name);
}

fn main() {}
