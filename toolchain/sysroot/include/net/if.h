#ifndef _NET_IF_H
#define _NET_IF_H

#include <sys/types.h>

#define IF_NAMESIZE  16
#define IFNAMSIZ     16

struct if_nameindex {
    unsigned int if_index;
    char        *if_name;
};

struct if_nameindex *if_nameindex(void);
void if_freenameindex(struct if_nameindex *ptr);
unsigned int if_nametoindex(const char *ifname);
char *if_indextoname(unsigned int ifindex, char *ifname);

#endif /* _NET_IF_H */
