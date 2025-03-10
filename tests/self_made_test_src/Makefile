CC = /opt/riscv/bin/riscv64-unknown-elf-gcc
OBJCOPY = /opt/riscv/bin/riscv64-unknown-elf-objcopy

OBJS = return_0.s

%.o: %.S Makefile
	$(CC) -c -o $@ $<

%.bin: %.o Makefile
	$(OBJCOPY) -O binary $< $@

