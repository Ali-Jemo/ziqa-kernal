#ifndef _REGEX_H
#define _REGEX_H

#include <sys/types.h>

typedef int regoff_t;
typedef struct {
    size_t re_nsub;
} regex_t;

typedef struct {
    regoff_t rm_so;
    regoff_t rm_eo;
} regmatch_t;

#define REG_EXTENDED 1
#define REG_ICASE    2
#define REG_NOSUB    4
#define REG_NEWLINE  8

#define REG_NOTBOL   1
#define REG_NOTEOL   2

extern int regcomp(regex_t *preg, const char *regex, int cflags);
extern int regexec(const regex_t *preg, const char *string, size_t nmatch, regmatch_t pmatch[], int eflags);
extern void regfree(regex_t *preg);

#endif
