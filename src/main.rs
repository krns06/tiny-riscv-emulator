use tiny_riscv_emulator::emulator::Emulator;

fn show_emulator(emulator: &Emulator) {
    println!("Current PC: 0x{:016x}", emulator.pc());
    println!("Regs: {:?}", emulator.regs());
    println!("memory: ...");
}

fn main() {
    let mut emulator = Emulator::default();

    emulator
        .load("tests/self_made_test_src/return_100.bin")
        .unwrap();

    emulator.run();
    show_emulator(&emulator);
}
