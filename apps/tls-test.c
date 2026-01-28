/*
 * TLS (Thread-Local Storage) test program
 * Tests basic TLS functionality on OXIDE OS
 */

#include <stdio.h>
#include <stdlib.h>

// Thread-local variables using GCC __thread extension
__thread int tls_int = 42;
__thread char tls_char = 'A';
__thread long tls_long = 0x1234567890ABCDEF;

// Structure in TLS
struct TlsData {
    int value;
    char name[16];
};

__thread struct TlsData tls_struct = {
    .value = 100,
    .name = "TLS_TEST"
};

// Function to test TLS access
void test_tls_access(void) {
    printf("=== TLS Access Test ===\n");

    // Read TLS variables
    printf("tls_int = %d (expected: 42)\n", tls_int);
    printf("tls_char = '%c' (expected: 'A')\n", tls_char);
    printf("tls_long = 0x%lx (expected: 0x1234567890ABCDEF)\n", tls_long);
    printf("tls_struct.value = %d (expected: 100)\n", tls_struct.value);
    printf("tls_struct.name = \"%s\" (expected: \"TLS_TEST\")\n", tls_struct.name);

    // Verify values
    int errors = 0;
    if (tls_int != 42) {
        printf("ERROR: tls_int mismatch!\n");
        errors++;
    }
    if (tls_char != 'A') {
        printf("ERROR: tls_char mismatch!\n");
        errors++;
    }
    if (tls_long != 0x1234567890ABCDEF) {
        printf("ERROR: tls_long mismatch!\n");
        errors++;
    }
    if (tls_struct.value != 100) {
        printf("ERROR: tls_struct.value mismatch!\n");
        errors++;
    }

    printf("\nRead test: %s\n", errors == 0 ? "PASS" : "FAIL");
}

// Function to test TLS writes
void test_tls_write(void) {
    printf("\n=== TLS Write Test ===\n");

    // Modify TLS variables
    tls_int = 99;
    tls_char = 'Z';
    tls_long = 0xDEADBEEFCAFEBABE;
    tls_struct.value = 200;

    printf("After modification:\n");
    printf("tls_int = %d (expected: 99)\n", tls_int);
    printf("tls_char = '%c' (expected: 'Z')\n", tls_char);
    printf("tls_long = 0x%lx (expected: 0xDEADBEEFCAFEBABE)\n", tls_long);
    printf("tls_struct.value = %d (expected: 200)\n", tls_struct.value);

    // Verify
    int errors = 0;
    if (tls_int != 99) {
        printf("ERROR: tls_int write failed!\n");
        errors++;
    }
    if (tls_char != 'Z') {
        printf("ERROR: tls_char write failed!\n");
        errors++;
    }
    if (tls_long != 0xDEADBEEFCAFEBABE) {
        printf("ERROR: tls_long write failed!\n");
        errors++;
    }
    if (tls_struct.value != 200) {
        printf("ERROR: tls_struct.value write failed!\n");
        errors++;
    }

    printf("Write test: %s\n", errors == 0 ? "PASS" : "FAIL");
}

// Get FS base register value (x86-64 specific)
static inline unsigned long get_fs_base(void) {
    unsigned long fs_base;
    // FS base is at FS:0
    __asm__ volatile (
        "mov %%fs:0, %0"
        : "=r" (fs_base)
    );
    return fs_base;
}

int main(int argc, char **argv) {
    printf("OXIDE TLS Test Program\n");
    printf("======================\n\n");

    // Print FS base register
    unsigned long fs = get_fs_base();
    printf("FS base register: 0x%lx\n", fs);

    // Check if FS base looks valid (should be in user space, non-zero for TLS)
    if (fs == 0) {
        printf("WARNING: FS base is 0 - TLS may not be initialized!\n");
    } else if (fs >= 0x0000800000000000UL) {
        printf("WARNING: FS base is in kernel space - this is wrong!\n");
    } else {
        printf("FS base looks valid (user space address)\n");
    }

    printf("\n");

    // Run tests
    test_tls_access();
    test_tls_write();

    printf("\n=== TLS Test Complete ===\n");
    return 0;
}
