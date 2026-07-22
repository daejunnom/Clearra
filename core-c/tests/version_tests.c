#include "../include/clearra_core.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define EXPECT_TRUE(EXPR)                                                   \
    do {                                                                    \
        if (!(EXPR)) {                                                      \
            fprintf(stderr, "%s:%d expected true\n", __FILE__, __LINE__);  \
            exit(1);                                                        \
        }                                                                   \
    } while (0)

#define EXPECT_INT(EXPR, EXPECTED)                                                   \
    do {                                                                             \
        int actual_value = (int)(EXPR);                                              \
        int expected_value = (int)(EXPECTED);                                        \
        if (actual_value != expected_value) {                                        \
            fprintf(stderr, "%s:%d expected %d but got %d\n", __FILE__, __LINE__,   \
                    expected_value, actual_value);                                   \
            exit(1);                                                                 \
        }                                                                            \
    } while (0)
static void abi_version_matches_header(void) {
    EXPECT_INT(clearra_core_abi_version(), CLEARRA_CORE_ABI_VERSION);
}static void version_string_is_non_empty(void) {
    const char *version = clearra_core_version();
    EXPECT_TRUE(version != NULL);
    EXPECT_TRUE(strlen(version) > 0);
}int main(void) {
    abi_version_matches_header();
    version_string_is_non_empty();
    puts("core-c version tests passed");
    return 0;
}