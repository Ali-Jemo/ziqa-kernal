#ifndef _TIME_H
#define _TIME_H

#include <sys/types.h>

typedef long time_t;

struct timespec {
    time_t tv_sec;
    long   tv_nsec;
};

struct tm {
    int tm_sec;
    int tm_min;
    int tm_hour;
    int tm_mday;
    int tm_mon;
    int tm_year;
    int tm_wday;
    int tm_yday;
    int tm_isdst;
};

extern time_t time(time_t *tloc);
extern struct tm *localtime(const time_t *timep);
extern struct tm *localtime_r(const time_t *timep, struct tm *result);
extern size_t strftime(char *s, size_t max, const char *format, const struct tm *tm);

#endif
