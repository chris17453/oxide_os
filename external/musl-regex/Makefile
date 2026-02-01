# Makefile for POSIX regex library (musl-based) for OXIDE OS
OXIDE_ROOT := $(shell cd ../.. && pwd)
TOOLCHAIN := $(OXIDE_ROOT)/toolchain
SYSROOT := $(TOOLCHAIN)/sysroot

CC := $(TOOLCHAIN)/bin/oxide-cc
AR := llvm-ar
RANLIB := llvm-ranlib

# Compiler flags
# OXIDE OS: Disable ALL optimizations due to compiler volatility issues
# -O0: No optimization to prevent pointer/inline breakage
# -fno-strict-aliasing: Prevent aggressive pointer optimization issues
# -fwrapv: Ensure proper signed overflow handling
# -fno-inline: Never inline functions
# -fno-builtin: Don't use builtin functions
CFLAGS := -O0 -fno-strict-aliasing -fwrapv -fno-inline -fno-builtin -I. -I$(SYSROOT)/include

OBJS := regcomp.o regexec.o regerror.o tre-mem.o

TARGET := libregex.a

all: $(TARGET) install

$(TARGET): $(OBJS)
	@echo "  [AR] $@"
	@$(AR) rcs $@ $^
	@$(RANLIB) $@

%.o: %.c
	@echo "  [CC] $<"
	@$(CC) $(CFLAGS) -c $< -o $@

install: $(TARGET)
	@echo "  [INSTALL] $(TARGET) -> $(SYSROOT)/lib/"
	@mkdir -p $(SYSROOT)/lib
	@cp $(TARGET) $(SYSROOT)/lib/

clean:
	@rm -f $(OBJS) $(TARGET)

.PHONY: all install clean
