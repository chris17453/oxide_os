#ifndef _LINUX_NETLINK_H
#define _LINUX_NETLINK_H

#include <sys/types.h>
#include <sys/socket.h>

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

struct sockaddr_nl {
    unsigned short nl_family;
    unsigned short nl_pad;
    unsigned int   nl_pid;
    unsigned int   nl_groups;
};

struct nlmsghdr {
    unsigned int nlmsg_len;
    unsigned short nlmsg_type;
    unsigned short nlmsg_flags;
    unsigned int nlmsg_seq;
    unsigned int nlmsg_pid;
};

#define NLM_F_REQUEST   1
#define NLM_F_MULTI     2
#define NLM_F_ACK       4
#define NLM_F_ECHO      8

#define NLMSG_ALIGNTO   4
#define NLMSG_ALIGN(len) (((len)+NLMSG_ALIGNTO-1) & ~(NLMSG_ALIGNTO-1))
#define NLMSG_HDRLEN    ((int)NLMSG_ALIGN(sizeof(struct nlmsghdr)))
#define NLMSG_LENGTH(len) ((len)+NLMSG_HDRLEN)
#define NLMSG_DATA(nlh) ((void*)(((char*)nlh) + NLMSG_HDRLEN))

#endif /* _LINUX_NETLINK_H */
