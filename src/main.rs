use tiny_riscv_emulator::emulator::Emulator;
const TEST_DIR: &str = "tests/isa/flats";

fn run_tests(emulator: &mut Emulator, name: &str, tests: &[&str]) {
    eprintln!("[info]: start {}", name);
    for test in tests {
        emulator.load(format!("{}/{}", TEST_DIR, test)).unwrap();

        emulator.run();

        // gp(3)が1であることを確認する。
        assert!(emulator.regs()[2] == 1);
        println!("[info]: Test {} was successful.", test);
    }
    eprintln!("[info]: end {}", name);
}

fn main() {
    let mut emulator = Emulator::default();

    // メモリをなぜかスタックに固定で確保する実装になっているのでそれを
    // ヒープにするまで$cargo rでテストを行うことにする。

    let ui_p_tests = [
        "rv64ui-p-add.bin",
        "rv64ui-p-addi.bin",
        "rv64ui-p-addiw.bin",
        "rv64ui-p-addw.bin",
        "rv64ui-p-and.bin",
        "rv64ui-p-andi.bin",
        "rv64ui-p-auipc.bin",
        "rv64ui-p-beq.bin",
        "rv64ui-p-bge.bin",
        "rv64ui-p-bgeu.bin",
        "rv64ui-p-blt.bin",
        "rv64ui-p-bltu.bin",
        "rv64ui-p-bne.bin",
        "rv64ui-p-fence_i.bin",
        "rv64ui-p-jal.bin",
        "rv64ui-p-jalr.bin",
        "rv64ui-p-lb.bin",
        "rv64ui-p-lbu.bin",
        "rv64ui-p-ld.bin",
        "rv64ui-p-ld_st.bin",
        "rv64ui-p-lh.bin",
        "rv64ui-p-lhu.bin",
        "rv64ui-p-lui.bin",
        "rv64ui-p-lw.bin",
        "rv64ui-p-lwu.bin",
        "rv64ui-p-ma_data.bin",
        "rv64ui-p-or.bin",
        "rv64ui-p-ori.bin",
        "rv64ui-p-sb.bin",
        "rv64ui-p-sd.bin",
        "rv64ui-p-sh.bin",
        "rv64ui-p-simple.bin",
        "rv64ui-p-sll.bin",
        "rv64ui-p-slli.bin",
        "rv64ui-p-slliw.bin",
        "rv64ui-p-sllw.bin",
        "rv64ui-p-slt.bin",
        "rv64ui-p-slti.bin",
        "rv64ui-p-sltiu.bin",
        "rv64ui-p-sltu.bin",
        "rv64ui-p-sra.bin",
        "rv64ui-p-srai.bin",
        "rv64ui-p-sraiw.bin",
        "rv64ui-p-sraw.bin",
        "rv64ui-p-srl.bin",
        "rv64ui-p-srli.bin",
        "rv64ui-p-srliw.bin",
        "rv64ui-p-srlw.bin",
        "rv64ui-p-st_ld.bin",
        "rv64ui-p-sub.bin",
        "rv64ui-p-subw.bin",
        "rv64ui-p-sw.bin",
        "rv64ui-p-xor.bin",
        "rv64ui-p-xori.bin",
    ];

    run_tests(&mut emulator, "rv64ui-p-*", &ui_p_tests);

    let um_p_tests = [
        "rv64um-p-div.bin",
        "rv64um-p-divu.bin",
        "rv64um-p-divuw.bin",
        "rv64um-p-divw.bin",
        "rv64um-p-mul.bin",
        "rv64um-p-mulh.bin",
        "rv64um-p-mulhsu.bin",
        "rv64um-p-mulhu.bin",
        "rv64um-p-mulw.bin",
    ];

    run_tests(&mut emulator, "rv64um-p-*", &um_p_tests);
}
