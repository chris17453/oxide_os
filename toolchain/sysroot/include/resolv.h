/* OXIDE OS resolv.h — DNS resolver interface
 * — ShadePacket: "glib's GResolver pokes at this. Stub it or watch meson cry."
 */

#ifndef _RESOLV_H
#define _RESOLV_H

#include <sys/types.h>
#include <netinet/in.h>
#include <arpa/nameser.h>

#ifdef __cplusplus
extern "C" {
#endif

#define MAXNS       3
#define MAXDNSRCH   6
#define MAXDFLSRCH  3

#define RES_INIT    0x00000001
#define RES_DEBUG   0x00000002
#define RES_RECURSE 0x00000040
#define RES_DEFNAMES 0x00000080
#define RES_STAYOPEN 0x00000100
#define RES_DNSRCH  0x00000200

struct __res_state {
    int retrans;
    int retry;
    unsigned long options;
    int nscount;
    struct sockaddr_in nsaddr_list[MAXNS];
    unsigned short id;
    char *dnsrch[MAXDNSRCH + 1];
    char defdname[256];
    unsigned long pfcode;
    unsigned ndots :4;
    unsigned nsort :4;
    char unused[3];
};

typedef struct __res_state *res_state;

extern struct __res_state _res;

int res_init(void);
int res_query(const char *dname, int __class, int __type,
              unsigned char *answer, int anslen);
int res_search(const char *dname, int __class, int __type,
               unsigned char *answer, int anslen);
int res_mkquery(int op, const char *dname, int __class, int __type,
                const unsigned char *data, int datalen,
                const unsigned char *newrr,
                unsigned char *buf, int buflen);
int res_send(const unsigned char *msg, int msglen,
             unsigned char *answer, int anslen);
int dn_comp(const char *exp_dn, unsigned char *comp_dn, int length,
            unsigned char **dnptrs, unsigned char **lastdnptr);
int dn_expand(const unsigned char *msg, const unsigned char *eomorig,
              const unsigned char *comp_dn, char *exp_dn, int length);

#ifdef __cplusplus
}
#endif

#endif /* _RESOLV_H */
