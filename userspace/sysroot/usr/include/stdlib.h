#ifndef _STDLIB_H
#define _STDLIB_H

#include <stddef.h>
#include <sys/types.h>

#define EXIT_SUCCESS 0
#define EXIT_FAILURE 1

extern void *malloc(size_t size);
extern void free(void *ptr);
extern void *calloc(size_t nmemb, size_t size);
extern void *realloc(void *ptr, size_t size);
extern void exit(int status);
extern char *getenv(const char *name);
extern int putenv(char *string);
extern int atoi(const char *nptr);
extern int system(const char *command);
extern void *bsearch(const void *key, const void *base, size_t nmemb, size_t size, int (*compar)(const void *, const void *));
extern void qsort(void *base, size_t nmemb, size_t size, int (*compar)(const void *, const void *));
extern unsigned long long strtoull(const char *nptr, char **endptr, int base);

extern pid_t getuid(void);
extern pid_t getgid(void);

#endif
