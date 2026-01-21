#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* Simple calculator for OXIDE OS */

double calculate(double a, double b, char op) {
    switch (op) {
        case '+': return a + b;
        case '-': return a - b;
        case '*': return a * b;
        case '/': 
            if (b == 0) {
                fprintf(stderr, "Error: Division by zero\n");
                exit(1);
            }
            return a / b;
        default:
            fprintf(stderr, "Error: Unknown operator '%c'\n", op);
            exit(1);
    }
}

int main(int argc, char *argv[]) {
    if (argc != 4) {
        fprintf(stderr, "Usage: %s <number> <op> <number>\n", argv[0]);
        fprintf(stderr, "Operators: + - * /\n");
        fprintf(stderr, "Example: %s 10 + 5\n", argv[0]);
        return 1;
    }

    double a = atof(argv[1]);
    char op = argv[2][0];
    double b = atof(argv[3]);

    double result = calculate(a, b, op);
    printf("%.2f %c %.2f = %.2f\n", a, op, b, result);

    return 0;
}
