CC = ../../bin/oxide-cc
CFLAGS = -O2 -Wall

all: echo

echo: echo.c
	$(CC) $(CFLAGS) -o echo echo.c

clean:
	rm -f echo *.o

.PHONY: all clean
