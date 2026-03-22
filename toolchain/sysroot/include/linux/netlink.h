/* OXIDE OS linux/netlink.h — Netlink socket interface */

#ifndef _LINUX_NETLINK_H
#define _LINUX_NETLINK_H

#include <sys/types.h>
#include <sys/socket.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Netlink protocols */
#define NETLINK_ROUTE     0
#define NETLINK_UNUSED    1
#define NETLINK_USERSOCK  2
#define NETLINK_FIREWALL  3
#define NETLINK_INET_DIAG 4
#define NETLINK_NFLOG     5
#define NETLINK_XFRM      6
#define NETLINK_SELINUX    7
#define NETLINK_ISCSI      8
#define NETLINK_AUDIT      9
#define NETLINK_FIB_LOOKUP 10
#define NETLINK_CONNECTOR  11
#define NETLINK_NETFILTER  12
#define NETLINK_IP6_FW     13
#define NETLINK_DNRTMSG    14
#define NETLINK_KOBJECT_UEVENT 15
#define NETLINK_GENERIC    16
#define NETLINK_CRYPTO     21

/* Protocol family */
#define PF_NETLINK      16
#define AF_NETLINK      PF_NETLINK

/* Netlink message header */
struct nlmsghdr {
    unsigned int nlmsg_len;
    unsigned short nlmsg_type;
    unsigned short nlmsg_flags;
    unsigned int nlmsg_seq;
    unsigned int nlmsg_pid;
};

struct sockaddr_nl {
    unsigned short nl_family;
    unsigned short nl_pad;
    unsigned int   nl_pid;
    unsigned int   nl_groups;
};

struct nlmsgerr {
    int error;
    struct nlmsghdr msg;
};

/* Generic routing message */
struct rtgenmsg {
    unsigned char rtgen_family;
};

/* Message flags */
#define NLM_F_REQUEST   0x01
#define NLM_F_MULTI     0x02
#define NLM_F_ACK       0x04
#define NLM_F_ECHO      0x08
#define NLM_F_ROOT      0x100
#define NLM_F_MATCH     0x200
#define NLM_F_DUMP      (NLM_F_ROOT | NLM_F_MATCH)

/* Message types */
#define NLMSG_NOOP      1
#define NLMSG_ERROR     2
#define NLMSG_DONE      3
#define NLMSG_OVERRUN   4

/* Alignment and access macros */
#define NLMSG_ALIGNTO   4
#define NLMSG_ALIGN(len)    (((len)+NLMSG_ALIGNTO-1) & ~(NLMSG_ALIGNTO-1))
#define NLMSG_HDRLEN        ((int)NLMSG_ALIGN(sizeof(struct nlmsghdr)))
#define NLMSG_LENGTH(len)   ((len)+NLMSG_HDRLEN)
#define NLMSG_SPACE(len)    NLMSG_ALIGN(NLMSG_LENGTH(len))
#define NLMSG_DATA(nlh)     ((void*)(((char*)(nlh)) + NLMSG_HDRLEN))
#define NLMSG_NEXT(nlh,len) ((len) -= NLMSG_ALIGN((nlh)->nlmsg_len), \
    (struct nlmsghdr*)(((char*)(nlh)) + NLMSG_ALIGN((nlh)->nlmsg_len)))
#define NLMSG_OK(nlh,len)   ((len) >= (int)sizeof(struct nlmsghdr) && \
    (nlh)->nlmsg_len >= sizeof(struct nlmsghdr) && \
    (nlh)->nlmsg_len <= (unsigned int)(len))

/* Multicast groups for rtnetlink */
#define RTMGRP_LINK          1
#define RTMGRP_NOTIFY        2
#define RTMGRP_NEIGH         4
#define RTMGRP_TC            8
#define RTMGRP_IPV4_IFADDR   0x10
#define RTMGRP_IPV4_MROUTE   0x20
#define RTMGRP_IPV4_ROUTE    0x40
#define RTMGRP_IPV6_IFADDR   0x100
#define RTMGRP_IPV6_MROUTE   0x200
#define RTMGRP_IPV6_ROUTE    0x400

#ifdef __cplusplus
}
#endif

#endif /* _LINUX_NETLINK_H */
