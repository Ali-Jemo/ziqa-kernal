# userspace/libposix/Makefile
#
# Builds libposix.a — the ZiqaKernel POSIX compatibility static library.
#
# Usage:
#   make              — build libposix.a
#   make clean        — remove build artifacts
#   make check        — syntax-check without linking
#
# Busybox integration:
#   Pass EXTRA_LDFLAGS="-L$(LIBPOSIX_DIR) -lposix" and
#        EXTRA_CFLAGS="-I$(LIBPOSIX_DIR)" when configuring BusyBox.
#
# Cross-compilation:
#   make CC=x86_64-linux-musl-gcc   (or your toolchain prefix)

CC      ?= gcc
AR      ?= ar
RANLIB  ?= ranlib

# Target bare-metal x86_64; use musl or a freestanding libc where available.
ARCH     := x86_64
TARGET   := $(ARCH)-linux-musl

# Compile flags:
#   -ffreestanding  — do not assume a hosted C environment
#   -nostdlib       — do not link against any stdlib (libposix IS the compat layer)
#   -fno-stack-protector — no SSP (__stack_chk_fail) in bare-metal context
#   -mno-red-zone   — disable red-zone (required for kernel-stack safety)
#   -O2             — optimise without LTO to keep ABI stable
CFLAGS  := -O2 -g \
            -std=c11 \
            -Wall -Wextra -Wpedantic \
            -ffreestanding \
            -fno-stack-protector \
            -mno-red-zone \
            -fno-omit-frame-pointer \
            -I.

# Source files
SRCS    := posix.c signals.c
OBJS    := $(SRCS:.c=.o)
LIB     := libposix.a

.PHONY: all clean check install

all: $(LIB)

$(LIB): $(OBJS)
	$(AR) rcs $@ $^
	$(RANLIB) $@
	@echo "[libposix] Built $(LIB) ($(shell wc -c < $(LIB)) bytes)"

%.o: %.c
	$(CC) $(CFLAGS) -c $< -o $@

# Syntax check only (no object output)
check:
	$(CC) $(CFLAGS) -fsyntax-only posix.c signals.c

clean:
	rm -f $(OBJS) $(LIB)

# Install into a sysroot for BusyBox / Bash linking
DESTDIR ?= ../../sysroot
install: $(LIB)
	install -d $(DESTDIR)/usr/lib $(DESTDIR)/usr/include
	install -m 644 $(LIB)      $(DESTDIR)/usr/lib/
	install -m 644 posix.h     $(DESTDIR)/usr/include/
	install -m 644 signals.h   $(DESTDIR)/usr/include/
	@echo "[libposix] Installed to $(DESTDIR)"
