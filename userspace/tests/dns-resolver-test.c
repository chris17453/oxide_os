/*
 * dns-resolver-test.c — DNS resolver test suite
 *
 * — ShadePacket: Tests res_query, res_mkquery, dn_expand, ns_initparse,
 * ns_parserr. Validates DNS query building, name compression/decompression,
 * and response parsing.
 *
 * NOTE: Tests 1-4 (query building and parsing) work offline.
 * Test 5+ (actual DNS resolution) requires network access.
 */

extern int res_query(const char *dname, int class, int type,
                     unsigned char *answer, int anslen);
extern int res_mkquery(int op, const char *dname, int class, int type,
                       const unsigned char *data, int datalen,
                       const unsigned char *newrr,
                       unsigned char *buf, int buflen);
extern int dn_expand(const unsigned char *msg, const unsigned char *eomorig,
                     const unsigned char *comp_dn, char *exp_dn, int length);
extern int ns_initparse(const unsigned char *msg, int msglen, void *handle);
extern int ns_parserr(void *handle, int section, int rrnum, void *rr);

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

static void print_num(long n) {
    char buf[20];
    int i = 0;
    if (n < 0) { print("-"); n = -n; }
    if (n == 0) { print("0"); return; }
    while (n > 0) { buf[i++] = '0' + (n % 10); n /= 10; }
    while (i > 0) { char c[2] = {buf[--i], 0}; print(c); }
}

/* DNS constants */
#define C_IN 1
#define T_A  1
#define T_AAAA 28

/* ns_sect enum values */
#define ns_s_qd 0  /* Question */
#define ns_s_an 1  /* Answer */

int main(void) {
    print("=== DNS Resolver Tests ===\n");

    /* Test 1: res_mkquery builds a valid DNS packet */
    print("\n--- Query Building ---\n");
    {
        unsigned char buf[512];
        int len = res_mkquery(0, "example.com", C_IN, T_A, 0, 0, 0, buf, sizeof(buf));
        test("res_mkquery returns positive length", len > 0);
        test("res_mkquery header is at least 12 bytes", len >= 12);

        if (len >= 12) {
            /* Check header: QDCOUNT should be 1 */
            int qdcount = (buf[4] << 8) | buf[5];
            test("res_mkquery QDCOUNT = 1", qdcount == 1);

            /* Check flags: RD bit set (byte 2, bit 0) */
            int flags = (buf[2] << 8) | buf[3];
            test("res_mkquery RD flag set", (flags & 0x0100) != 0);
            test("res_mkquery QR=0 (query)", (flags & 0x8000) == 0);

            /* Check QNAME encoding: "example" label should start at byte 12 */
            test("res_mkquery QNAME label len 'example'=7", buf[12] == 7);
            test("res_mkquery QNAME first char 'e'", buf[13] == 'e');
        }
    }

    /* Test 2: dn_expand decompresses simple (uncompressed) names */
    print("\n--- Name Decompression ---\n");
    {
        /* Construct a minimal DNS message with "example.com" in QNAME */
        unsigned char msg[64];
        int pos = 0;
        /* Header (12 bytes, zeroed) */
        for (int i = 0; i < 12; i++) msg[pos++] = 0;
        /* QNAME: \x07example\x03com\x00 */
        msg[pos++] = 7; /* "example" */
        msg[pos++]='e'; msg[pos++]='x'; msg[pos++]='a'; msg[pos++]='m';
        msg[pos++]='p'; msg[pos++]='l'; msg[pos++]='e';
        msg[pos++] = 3; /* "com" */
        msg[pos++]='c'; msg[pos++]='o'; msg[pos++]='m';
        msg[pos++] = 0; /* end */
        int msg_len = pos;

        char expanded[256] = {0};
        int consumed = dn_expand(msg, msg + msg_len, msg + 12, expanded, sizeof(expanded));

        test("dn_expand returns positive consumed bytes", consumed > 0);
        test("dn_expand consumed = 13 bytes", consumed == 13); /* 1+7+1+3+1 */

        /* Check expanded name */
        int match = 1;
        const char *expected = "example.com";
        for (int i = 0; expected[i]; i++) {
            if (expanded[i] != expected[i]) { match = 0; break; }
        }
        test("dn_expand produces 'example.com'", match);
    }

    /* Test 3: dn_expand handles compression pointers */
    {
        unsigned char msg[64];
        int pos = 0;
        /* Header */
        for (int i = 0; i < 12; i++) msg[pos++] = 0;
        /* QNAME at offset 12: \x03foo\x03bar\x00 */
        int name1_start = pos;
        msg[pos++] = 3; msg[pos++]='f'; msg[pos++]='o'; msg[pos++]='o';
        msg[pos++] = 3; msg[pos++]='b'; msg[pos++]='a'; msg[pos++]='r';
        msg[pos++] = 0;
        /* Second name at current pos: \x03baz + compression pointer to "bar" (offset 16) */
        int name2_start = pos;
        msg[pos++] = 3; msg[pos++]='b'; msg[pos++]='a'; msg[pos++]='z';
        msg[pos++] = 0xC0; msg[pos++] = 16; /* pointer to offset 16 = ".bar\0" */
        int msg_len = pos;

        char expanded[256] = {0};
        int consumed = dn_expand(msg, msg + msg_len, msg + name2_start, expanded, sizeof(expanded));

        test("dn_expand with compression pointer succeeds", consumed > 0);
        /* Should produce "baz.bar" */
        int match = 1;
        const char *expected = "baz.bar";
        for (int i = 0; expected[i]; i++) {
            if (expanded[i] != expected[i]) { match = 0; break; }
        }
        test("dn_expand with pointer produces 'baz.bar'", match);
    }

    /* Test 4: ns_initparse + ns_parserr on a synthetic response */
    print("\n--- Message Parsing ---\n");
    {
        /* Build a synthetic DNS response:
         * Header: ID=0x1234, QR=1, QDCOUNT=1, ANCOUNT=1
         * Question: example.com A IN
         * Answer: example.com A IN TTL=300 RDATA=93.184.216.34
         */
        unsigned char msg[128];
        int pos = 0;

        /* Header */
        msg[pos++] = 0x12; msg[pos++] = 0x34; /* ID */
        msg[pos++] = 0x81; msg[pos++] = 0x80; /* Flags: QR=1, RD=1, RA=1 */
        msg[pos++] = 0; msg[pos++] = 1; /* QDCOUNT=1 */
        msg[pos++] = 0; msg[pos++] = 1; /* ANCOUNT=1 */
        msg[pos++] = 0; msg[pos++] = 0; /* NSCOUNT=0 */
        msg[pos++] = 0; msg[pos++] = 0; /* ARCOUNT=0 */

        /* Question: example.com */
        msg[pos++] = 7;
        msg[pos++]='e'; msg[pos++]='x'; msg[pos++]='a'; msg[pos++]='m';
        msg[pos++]='p'; msg[pos++]='l'; msg[pos++]='e';
        msg[pos++] = 3;
        msg[pos++]='c'; msg[pos++]='o'; msg[pos++]='m';
        msg[pos++] = 0;
        msg[pos++] = 0; msg[pos++] = 1; /* QTYPE=A */
        msg[pos++] = 0; msg[pos++] = 1; /* QCLASS=IN */

        /* Answer: compression pointer to question name + A record */
        msg[pos++] = 0xC0; msg[pos++] = 12; /* Name pointer to offset 12 */
        msg[pos++] = 0; msg[pos++] = 1;  /* TYPE=A */
        msg[pos++] = 0; msg[pos++] = 1;  /* CLASS=IN */
        msg[pos++] = 0; msg[pos++] = 0; msg[pos++] = 1; msg[pos++] = 0x2C; /* TTL=300 */
        msg[pos++] = 0; msg[pos++] = 4;  /* RDLENGTH=4 */
        msg[pos++] = 93; msg[pos++] = 184; msg[pos++] = 216; msg[pos++] = 34; /* 93.184.216.34 */

        int msg_len = pos;

        /* ns_initparse */
        unsigned char handle[256]; /* Opaque handle (ns_msg struct) */
        int rc = ns_initparse(msg, msg_len, handle);
        test("ns_initparse succeeds", rc == 0);

        /* ns_parserr — read the answer record */
        unsigned char rr[512]; /* Opaque rr struct */
        rc = ns_parserr(handle, ns_s_an, 0, rr);
        test("ns_parserr(answer, 0) succeeds", rc == 0);

        if (rc == 0) {
            /* The rr struct layout: name[256], type(u16), class(u16), ttl(u32), rdlen(u16), rdata(*) */
            unsigned short rr_type = (rr[256] << 8) | rr[257];
            unsigned short rr_class = (rr[258] << 8) | rr[259];
            unsigned int rr_ttl = ((unsigned int)rr[260] << 24) | ((unsigned int)rr[261] << 16) |
                                  ((unsigned int)rr[262] << 8) | rr[263];
            unsigned short rr_rdlen = (rr[264] << 8) | rr[265];

            test("ns_parserr TYPE = A (1)", rr_type == 1);
            test("ns_parserr CLASS = IN (1)", rr_class == 1);
            test("ns_parserr TTL = 300", rr_ttl == 300);
            test("ns_parserr RDLENGTH = 4", rr_rdlen == 4);

            /* Check RDATA (IP address) via pointer at offset 266-273 */
            unsigned char *rdata = *(unsigned char **)(rr + 266);
            if (rdata) {
                test("ns_parserr RDATA[0] = 93", rdata[0] == 93);
                test("ns_parserr RDATA[1] = 184", rdata[1] == 184);
                test("ns_parserr RDATA[2] = 216", rdata[2] == 216);
                test("ns_parserr RDATA[3] = 34", rdata[3] == 34);
            }
        }

        /* Verify ns_parserr returns -1 for out-of-range rrnum */
        rc = ns_parserr(handle, ns_s_an, 1, rr);
        test("ns_parserr(answer, 1) fails (only 1 answer)", rc == -1);
    }

    /* Summary */
    print("\n=== DNS Resolver Results: ");
    print_num(tests_passed);
    print("/");
    print_num(tests_run);
    print(" passed ===\n");

    return tests_passed == tests_run ? 0 : 1;
}
