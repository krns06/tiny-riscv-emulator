# tiny-riscv-emulator
趣味で作成している軽量なriscvのエミュレータ。

# 特徴
* rv64ima_zicsr_zifenceiをサポート予定
* リトルエンディアンのみサポート

# 目標
- [x] riscv-testsのrv64ui-p-*を通す。
- [x] riscv-testsのrv64um-p-*を通す。
- [x] riscv-testsのrv64ua-p-*を通す。
- [x] riscv-testsのrv64uc-p-rvcを通す。
- [x] riscv-testsのrv64mi-p-*(breakpoint, sbreak, instret_overflow, zicntr, pmpaddrを除く)を通す。
- [x] riscv-testsのrv64si-p-*(dirty, icache-alias, sbreakを除く)を通す。
- [ ] riscv-testsのrv64u{i,a,m,c}-v-*.binを通す。
- [ ] xv6を動かす。
- [ ] Linuxを動かす。
