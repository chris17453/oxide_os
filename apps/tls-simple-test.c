// Simple test program with TLS
#include <stdio.h>

__thread int tls_var = 42;

int main(int argc, char **argv) {
    printf("TLS test: tls_var = %d\n", tls_var);
    tls_var = 100;
    printf("TLS test: after write tls_var = %d\n", tls_var);
    return 0;
}
