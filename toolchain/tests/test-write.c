#include <unistd.h>
#include <string.h>

int main() {
    const char *msg = "Test write to stderr\n";
    write(2, msg, strlen(msg));
    write(2, "Line 2\n", 7);
    write(2, "Line 3\n", 7);
    return 0;
}
