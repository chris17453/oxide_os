CC = ../../bin/oxide-cc
CFLAGS = -O2 -Wall

all: calculator

calculator: calculator.c
	$(CC) $(CFLAGS) -o calculator calculator.c -lm

clean:
	rm -f calculator *.o

.PHONY: all clean
