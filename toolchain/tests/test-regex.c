/*
 * POSIX regex test for OXIDE OS
 * Tests regex compilation, execution, and error handling
 */

#include <stdio.h>
#include <string.h>
#include <regex.h>
#include <stdlib.h>

#define TEST_PASS "\x1b[32mPASS\x1b[0m"
#define TEST_FAIL "\x1b[31mFAIL\x1b[0m"

int test_count = 0;
int pass_count = 0;

void test_regex(const char *pattern, const char *text, int cflags, int expected_match, const char *test_name) {
    regex_t regex;
    int ret;
    char errbuf[100];

    test_count++;
    printf("Test %d: %s\n", test_count, test_name);
    printf("  Pattern: '%s', Text: '%s'\n", pattern, text);
    printf("  [DEBUG] About to call regcomp...\n");
    fflush(stdout);

    ret = regcomp(&regex, pattern, cflags);
    printf("  [DEBUG] regcomp returned: %d\n", ret);
    fflush(stdout);
    if (ret != 0) {
        regerror(ret, &regex, errbuf, sizeof(errbuf));
        printf("  %s: regcomp failed: %s\n", TEST_FAIL, errbuf);
        return;
    }

    ret = regexec(&regex, text, 0, NULL, 0);

    if ((ret == 0 && expected_match) || (ret == REG_NOMATCH && !expected_match)) {
        printf("  %s: Match result correct (%s)\n", TEST_PASS, ret == 0 ? "matched" : "no match");
        pass_count++;
    } else {
        printf("  %s: Expected %s, got %s\n", TEST_FAIL,
               expected_match ? "match" : "no match",
               ret == 0 ? "match" : "no match");
    }

    regfree(&regex);
    printf("\n");
}

void test_submatch(const char *pattern, const char *text, int expected_matches) {
    regex_t regex;
    regmatch_t matches[10];
    int ret;

    test_count++;
    printf("Test %d: Submatch test\n", test_count);
    printf("  Pattern: '%s', Text: '%s'\n", pattern, text);

    ret = regcomp(&regex, pattern, REG_EXTENDED);
    if (ret != 0) {
        printf("  %s: regcomp failed\n", TEST_FAIL);
        return;
    }

    ret = regexec(&regex, text, 10, matches, 0);
    if (ret == 0) {
        int i;
        printf("  %s: Matched\n", TEST_PASS);
        for (i = 0; i < expected_matches && matches[i].rm_so != -1; i++) {
            printf("    Match %d: offset %ld-%ld: '", i,
                   (long)matches[i].rm_so, (long)matches[i].rm_eo);
            fwrite(text + matches[i].rm_so, 1, matches[i].rm_eo - matches[i].rm_so, stdout);
            printf("'\n");
        }
        if (i == expected_matches) {
            pass_count++;
        } else {
            printf("  %s: Expected %d matches, got %d\n", TEST_FAIL, expected_matches, i);
        }
    } else {
        printf("  %s: No match\n", TEST_FAIL);
    }

    regfree(&regex);
    printf("\n");
}

int main(void) {
    printf("=== OXIDE OS POSIX Regex Test Suite ===\n\n");

    /* Test 1: Literal matching */
    test_regex("hello", "hello world", REG_EXTENDED, 1, "Literal match");

    /* Test 2: Literal non-match */
    test_regex("goodbye", "hello world", REG_EXTENDED, 0, "Literal non-match");

    /* Test 3: Character class */
    test_regex("[a-z]+", "abc123", REG_EXTENDED, 1, "Character class [a-z]+");

    /* Test 4: Digit class */
    test_regex("[0-9]+", "test123", REG_EXTENDED, 1, "Digit class [0-9]+");

    /* Test 5: Quantifiers - + */
    test_regex("a+b", "aaab", REG_EXTENDED, 1, "Quantifier + (one or more)");

    /* Test 6: Quantifiers - * */
    test_regex("a*b", "b", REG_EXTENDED, 1, "Quantifier * (zero or more)");

    /* Test 7: Quantifiers - ? */
    test_regex("colou?r", "color", REG_EXTENDED, 1, "Quantifier ? (optional)");
    test_regex("colou?r", "colour", REG_EXTENDED, 1, "Quantifier ? (optional, variant)");

    /* Test 8: Anchors - ^ */
    test_regex("^start", "start of line", REG_EXTENDED, 1, "Anchor ^ (start)");
    test_regex("^start", "not start", REG_EXTENDED, 0, "Anchor ^ (start, negative)");

    /* Test 9: Anchors - $ */
    test_regex("end$", "line end", REG_EXTENDED, 1, "Anchor $ (end)");
    test_regex("end$", "end not", REG_EXTENDED, 0, "Anchor $ (end, negative)");

    /* Test 10: Alternation */
    test_regex("cat|dog", "I have a dog", REG_EXTENDED, 1, "Alternation (cat|dog)");

    /* Test 11: Case insensitive */
    test_regex("HELLO", "hello world", REG_EXTENDED | REG_ICASE, 1, "Case insensitive");

    /* Test 12: Subexpression matching */
    test_submatch("([a-z]+)([0-9]+)", "abc123def", 3);

    /* Test 13: Word boundary (using basic regex) */
    test_regex("\\<word\\>", "find word here", 0, 1, "Word boundary");

    /* Test 14: Escaped special chars */
    test_regex("\\.", "test.txt", 0, 1, "Escaped dot");

    /* Summary */
    printf("=== Test Summary ===\n");
    printf("Passed: %d/%d\n", pass_count, test_count);

    if (pass_count == test_count) {
        printf("\x1b[32mAll tests passed!\x1b[0m\n");
        return 0;
    } else {
        printf("\x1b[31m%d tests failed\x1b[0m\n", test_count - pass_count);
        return 1;
    }
}
