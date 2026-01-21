# Example: Hello World for OXIDE OS

CC = ../../bin/oxide-cc
CFLAGS = -O2 -Wall -g

all: hello

hello: hello.c
	$(CC) $(CFLAGS) -o hello hello.c

clean:
	rm -f hello *.o

test: hello
	@echo "Binary created: hello"
	@file hello || true
	@size hello || true

.PHONY: all clean test
