#ifndef _LINUX_TIPC_H
#define _LINUX_TIPC_H

#include <stdint.h>

#define TIPC_ADDR_NAMESEQ  1
#define TIPC_ADDR_NAME     2
#define TIPC_ADDR_ID       3

struct tipc_name {
    uint32_t type;
    uint32_t instance;
};

struct tipc_name_seq {
    uint32_t type;
    uint32_t lower;
    uint32_t upper;
};

struct sockaddr_tipc {
    unsigned short family;
    unsigned char addrtype;
    signed char scope;
    union {
        struct tipc_name_seq nameseq;
        struct { struct tipc_name name; uint32_t domain; } name;
        struct { uint32_t node; uint32_t ref; } id;
    } addr;
};

#endif /* _LINUX_TIPC_H */
