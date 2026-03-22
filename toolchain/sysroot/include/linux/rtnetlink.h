/* OXIDE OS linux/rtnetlink.h — routing netlink interface
 * — ShadePacket: glib's network monitor uses netlink to detect interface changes.
 * This provides the minimum constants and structs glib needs.
 */

#ifndef _LINUX_RTNETLINK_H
#define _LINUX_RTNETLINK_H

#include <linux/netlink.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Routing/link message types */
#define RTM_NEWLINK     16
#define RTM_DELLINK     17
#define RTM_GETLINK     18
#define RTM_NEWADDR     20
#define RTM_DELADDR     21
#define RTM_GETADDR     22
#define RTM_NEWROUTE    24
#define RTM_DELROUTE    25
#define RTM_GETROUTE    26

/* Interface info message */
struct ifinfomsg {
    unsigned char  ifi_family;
    unsigned char  __ifi_pad;
    unsigned short ifi_type;
    int            ifi_index;
    unsigned int   ifi_flags;
    unsigned int   ifi_change;
};

/* Interface address message */
struct ifaddrmsg {
    unsigned char ifa_family;
    unsigned char ifa_prefixlen;
    unsigned char ifa_flags;
    unsigned char ifa_scope;
    unsigned int  ifa_index;
};

/* Routing message */
struct rtmsg {
    unsigned char rtm_family;
    unsigned char rtm_dst_len;
    unsigned char rtm_src_len;
    unsigned char rtm_tos;
    unsigned char rtm_table;
    unsigned char rtm_protocol;
    unsigned char rtm_scope;
    unsigned char rtm_type;
    unsigned int  rtm_flags;
};

/* Route attributes */
#define RTA_UNSPEC  0
#define RTA_DST     1
#define RTA_SRC     2
#define RTA_IIF     3
#define RTA_OIF     4
#define RTA_GATEWAY 5
#define RTA_PRIORITY 6

/* Route types */
#define RTN_UNSPEC      0
#define RTN_UNICAST     1
#define RTN_LOCAL       2
#define RTN_BROADCAST   3
#define RTN_ANYCAST     4
#define RTN_MULTICAST   5
#define RTN_BLACKHOLE   6
#define RTN_UNREACHABLE 7
#define RTN_PROHIBIT    8
#define RTN_THROW       9
#define RTN_NAT         10

/* Route protocols */
#define RTPROT_UNSPEC   0
#define RTPROT_REDIRECT 1
#define RTPROT_KERNEL   2
#define RTPROT_BOOT     3
#define RTPROT_STATIC   4

/* Route scopes */
#define RT_SCOPE_UNIVERSE 0
#define RT_SCOPE_SITE     200
#define RT_SCOPE_LINK     253
#define RT_SCOPE_HOST     254
#define RT_SCOPE_NOWHERE  255

/* Routing attribute macros */
#define RTA_ALIGNTO     4
#define RTA_ALIGN(len)  (((len)+RTA_ALIGNTO-1) & ~(RTA_ALIGNTO-1))
#define RTA_LENGTH(len) (RTA_ALIGN(sizeof(struct rtattr)) + (len))
#define RTA_DATA(rta)   ((void*)(((char*)(rta)) + RTA_LENGTH(0)))
#define RTA_OK(rta,len) ((len) >= (int)sizeof(struct rtattr) && \
                          (rta)->rta_len >= sizeof(struct rtattr) && \
                          (rta)->rta_len <= (unsigned int)(len))
#define RTA_NEXT(rta,attrlen) ((attrlen) -= RTA_ALIGN((rta)->rta_len), \
                                (struct rtattr*)(((char*)(rta)) + RTA_ALIGN((rta)->rta_len)))

struct rtattr {
    unsigned short rta_len;
    unsigned short rta_type;
};

/* IFA attributes */
#define IFA_UNSPEC    0
#define IFA_ADDRESS   1
#define IFA_LOCAL     2
#define IFA_LABEL     3
#define IFA_BROADCAST 4

#ifdef __cplusplus
}
#endif

#endif /* _LINUX_RTNETLINK_H */
