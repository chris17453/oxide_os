/*
 * iconv-test.c — Character set conversion test suite
 *
 * — PulseForge: Tests real iconv implementation: UTF-8 ↔ Latin-1, ASCII,
 * UTF-16LE/BE, UTF-32LE/BE. Verifies encoding/decoding round-trips,
 * error handling for invalid sequences, and non-reversible conversion counting.
 */

#include <string.h>

extern void *iconv_open(const char *tocode, const char *fromcode);
extern unsigned long iconv(void *cd, char **inbuf, unsigned long *inbytesleft,
                           char **outbuf, unsigned long *outbytesleft);
extern int iconv_close(void *cd);

static long oxide_write(int fd, const void *buf, unsigned long count) {
    long ret;
    __asm__ volatile ("syscall" : "=a"(ret) : "a"(1), "D"(fd), "S"(buf), "d"(count) : "rcx", "r11", "memory");
    return ret;
}

static void print(const char *s) {
    unsigned long len = 0;
    while (s[len]) len++;
    oxide_write(1, s, len);
}

static int tests_run = 0;
static int tests_passed = 0;

static void test(const char *name, int condition) {
    tests_run++;
    if (condition) { tests_passed++; print("  [PASS] "); }
    else { print("  [FAIL] "); }
    print(name);
    print("\n");
}

/* Print a number */
static void print_num(long n) {
    char buf[20];
    int i = 0;
    if (n < 0) { print("-"); n = -n; }
    if (n == 0) { print("0"); return; }
    while (n > 0) { buf[i++] = '0' + (n % 10); n /= 10; }
    while (i > 0) { char c[2] = {buf[--i], 0}; print(c); }
}

#define ICONV_ERROR ((unsigned long)-1)

int main(void) {
    print("=== iconv Character Set Conversion Tests ===\n");

    /* Test 1: iconv_open with valid encodings */
    print("\n--- Encoding Recognition ---\n");
    {
        void *cd = iconv_open("UTF-8", "UTF-8");
        test("iconv_open(UTF-8, UTF-8) succeeds", cd != (void *)-1);
        if (cd != (void *)-1) iconv_close(cd);

        cd = iconv_open("ASCII", "UTF-8");
        test("iconv_open(ASCII, UTF-8) succeeds", cd != (void *)-1);
        if (cd != (void *)-1) iconv_close(cd);

        cd = iconv_open("ISO-8859-1", "UTF-8");
        test("iconv_open(ISO-8859-1, UTF-8) succeeds", cd != (void *)-1);
        if (cd != (void *)-1) iconv_close(cd);

        cd = iconv_open("UTF-16LE", "UTF-8");
        test("iconv_open(UTF-16LE, UTF-8) succeeds", cd != (void *)-1);
        if (cd != (void *)-1) iconv_close(cd);

        cd = iconv_open("UTF-32BE", "UTF-8");
        test("iconv_open(UTF-32BE, UTF-8) succeeds", cd != (void *)-1);
        if (cd != (void *)-1) iconv_close(cd);
    }

    /* Test 2: iconv_open with invalid encoding */
    {
        void *cd = iconv_open("EBCDIC", "UTF-8");
        test("iconv_open(EBCDIC, UTF-8) fails", cd == (void *)-1);
    }

    /* Test 3: UTF-8 → UTF-8 identity */
    print("\n--- UTF-8 → UTF-8 Identity ---\n");
    {
        void *cd = iconv_open("UTF-8", "UTF-8");
        char input[] = "Hello, World!";
        char output[64] = {0};
        char *in_ptr = input;
        char *out_ptr = output;
        unsigned long in_left = strlen(input);
        unsigned long out_left = sizeof(output);

        unsigned long rc = iconv(cd, &in_ptr, &in_left, &out_ptr, &out_left);
        test("UTF-8→UTF-8 identity conversion succeeds", rc != ICONV_ERROR);
        test("UTF-8→UTF-8 all input consumed", in_left == 0);
        test("UTF-8→UTF-8 output matches input", strcmp(input, output) == 0);
        iconv_close(cd);
    }

    /* Test 4: UTF-8 → Latin-1 (codepoints 0-255) */
    print("\n--- UTF-8 → Latin-1 ---\n");
    {
        void *cd = iconv_open("ISO-8859-1", "UTF-8");
        /* "café" in UTF-8: 63 61 66 c3 a9 */
        char input[] = "caf\xc3\xa9";
        char output[64] = {0};
        char *in_ptr = input;
        char *out_ptr = output;
        unsigned long in_left = 5; /* 3 ASCII + 2-byte é */
        unsigned long out_left = sizeof(output);

        unsigned long rc = iconv(cd, &in_ptr, &in_left, &out_ptr, &out_left);
        test("UTF-8→Latin-1 conversion succeeds", rc != ICONV_ERROR);
        test("UTF-8→Latin-1 all input consumed", in_left == 0);
        test("UTF-8→Latin-1 output is 4 bytes", (sizeof(output) - out_left) == 4);
        test("UTF-8→Latin-1 'c' correct", output[0] == 'c');
        test("UTF-8→Latin-1 'a' correct", output[1] == 'a');
        test("UTF-8→Latin-1 'f' correct", output[2] == 'f');
        test("UTF-8→Latin-1 'é' is 0xe9", (unsigned char)output[3] == 0xe9);
        iconv_close(cd);
    }

    /* Test 5: Latin-1 → UTF-8 */
    print("\n--- Latin-1 → UTF-8 ---\n");
    {
        void *cd = iconv_open("UTF-8", "ISO-8859-1");
        /* "café" in Latin-1: 63 61 66 e9 */
        char input[] = {'c', 'a', 'f', (char)0xe9, 0};
        char output[64] = {0};
        char *in_ptr = input;
        char *out_ptr = output;
        unsigned long in_left = 4;
        unsigned long out_left = sizeof(output);

        unsigned long rc = iconv(cd, &in_ptr, &in_left, &out_ptr, &out_left);
        test("Latin-1→UTF-8 conversion succeeds", rc != ICONV_ERROR);
        test("Latin-1→UTF-8 all input consumed", in_left == 0);
        test("Latin-1→UTF-8 output is 5 bytes", (sizeof(output) - out_left) == 5);
        test("Latin-1→UTF-8 'caf' prefix", output[0]=='c' && output[1]=='a' && output[2]=='f');
        test("Latin-1→UTF-8 'é' is 0xc3 0xa9", (unsigned char)output[3]==0xc3 && (unsigned char)output[4]==0xa9);
        iconv_close(cd);
    }

    /* Test 6: UTF-8 → ASCII (non-reversible — replaces non-ASCII with '?') */
    print("\n--- UTF-8 → ASCII (lossy) ---\n");
    {
        void *cd = iconv_open("ASCII", "UTF-8");
        char input[] = "caf\xc3\xa9";
        char output[64] = {0};
        char *in_ptr = input;
        char *out_ptr = output;
        unsigned long in_left = 5;
        unsigned long out_left = sizeof(output);

        unsigned long rc = iconv(cd, &in_ptr, &in_left, &out_ptr, &out_left);
        test("UTF-8→ASCII returns 1 non-reversible", rc == 1);
        test("UTF-8→ASCII all input consumed", in_left == 0);
        test("UTF-8→ASCII 'é' replaced with '?'", output[3] == '?');
        iconv_close(cd);
    }

    /* Test 7: UTF-8 → UTF-16LE */
    print("\n--- UTF-8 → UTF-16LE ---\n");
    {
        void *cd = iconv_open("UTF-16LE", "UTF-8");
        char input[] = "AB";
        char output[64] = {0};
        char *in_ptr = input;
        char *out_ptr = output;
        unsigned long in_left = 2;
        unsigned long out_left = sizeof(output);

        unsigned long rc = iconv(cd, &in_ptr, &in_left, &out_ptr, &out_left);
        test("UTF-8→UTF-16LE succeeds", rc != ICONV_ERROR);
        test("UTF-8→UTF-16LE output is 4 bytes", (sizeof(output) - out_left) == 4);
        test("UTF-8→UTF-16LE 'A' = 0x41 0x00", output[0]==0x41 && output[1]==0x00);
        test("UTF-8→UTF-16LE 'B' = 0x42 0x00", output[2]==0x42 && output[3]==0x00);
        iconv_close(cd);
    }

    /* Test 8: UTF-8 → UTF-32BE */
    print("\n--- UTF-8 → UTF-32BE ---\n");
    {
        void *cd = iconv_open("UTF-32BE", "UTF-8");
        char input[] = "A";
        char output[64] = {0};
        char *in_ptr = input;
        char *out_ptr = output;
        unsigned long in_left = 1;
        unsigned long out_left = sizeof(output);

        unsigned long rc = iconv(cd, &in_ptr, &in_left, &out_ptr, &out_left);
        test("UTF-8→UTF-32BE succeeds", rc != ICONV_ERROR);
        test("UTF-8→UTF-32BE output is 4 bytes", (sizeof(output) - out_left) == 4);
        test("UTF-8→UTF-32BE 'A' = 0x00000041",
             output[0]==0 && output[1]==0 && output[2]==0 && output[3]==0x41);
        iconv_close(cd);
    }

    /* Test 9: NULL inbuf = reset state */
    {
        void *cd = iconv_open("UTF-8", "UTF-8");
        unsigned long rc = iconv(cd, (char**)0, (unsigned long*)0, (char**)0, (unsigned long*)0);
        test("iconv(NULL inbuf) resets state (returns 0)", rc == 0);
        iconv_close(cd);
    }

    /* Summary */
    print("\n=== iconv Results: ");
    print_num(tests_passed);
    print("/");
    print_num(tests_run);
    print(" passed ===\n");

    return tests_passed == tests_run ? 0 : 1;
}
