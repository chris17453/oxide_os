# Build the init user program

TARGET = init.elf

AS = as
LD = ld

ASFLAGS = --64
LDFLAGS = -T linker.ld -nostdlib

.PHONY: all clean

all: $(TARGET)

$(TARGET): init.o
	$(LD) $(LDFLAGS) -o $@ $^

init.o: init.S
	$(AS) $(ASFLAGS) -o $@ $<

clean:
	rm -f *.o $(TARGET)
