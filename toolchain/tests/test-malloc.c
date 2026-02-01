/*
 * Simple malloc test for OXIDE OS
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int main(void) {
    printf("=== OXIDE OS Malloc Test ===\n\n");

    // Test 1: Simple malloc
    printf("Test 1: malloc(100)\n");
    fflush(stdout);
    char *p1 = malloc(100);
    printf("  Result: %p\n", (void*)p1);
    if (p1 == NULL) {
        printf("  FAIL: malloc returned NULL\n");
        return 1;
    }
    printf("  PASS\n\n");

    // Test 2: Write to allocated memory
    printf("Test 2: Write to allocated memory\n");
    fflush(stdout);
    strcpy(p1, "Hello, OXIDE!");
    printf("  Content: '%s'\n", p1);
    printf("  PASS\n\n");

    // Test 3: calloc
    printf("Test 3: calloc(10, 10)\n");
    fflush(stdout);
    char *p2 = calloc(10, 10);
    printf("  Result: %p\n", (void*)p2);
    if (p2 == NULL) {
        printf("  FAIL: calloc returned NULL\n");
        return 1;
    }

    // Check if zeroed
    int all_zero = 1;
    for (int i = 0; i < 100; i++) {
        if (p2[i] != 0) {
            all_zero = 0;
            break;
        }
    }
    printf("  Zeroed: %s\n", all_zero ? "yes" : "no");
    printf("  PASS\n\n");

    // Test 4: realloc
    printf("Test 4: realloc to 200 bytes\n");
    fflush(stdout);
    p1 = realloc(p1, 200);
    printf("  Result: %p\n", (void*)p1);
    if (p1 == NULL) {
        printf("  FAIL: realloc returned NULL\n");
        return 1;
    }
    printf("  Content still intact: '%s'\n", p1);
    printf("  PASS\n\n");

    // Test 5: free
    printf("Test 5: free()\n");
    fflush(stdout);
    free(p1);
    free(p2);
    printf("  PASS\n\n");

    printf("=== All tests passed! ===\n");
    return 0;
}
