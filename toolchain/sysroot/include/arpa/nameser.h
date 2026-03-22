/* OXIDE OS arpa/nameser.h — DNS name server definitions
 * — ShadePacket: "glib wants this for GResolver. Minimal but enough to not choke."
 */

#ifndef _ARPA_NAMESER_H
#define _ARPA_NAMESER_H

#include <stddef.h>
#include <stdint.h>
#include <arpa/nameser_compat.h>

/* DNS message header */
typedef struct {
    unsigned id     :16;
    unsigned rd     :1;
    unsigned tc     :1;
    unsigned aa     :1;
    unsigned opcode :4;
    unsigned qr     :1;
    unsigned rcode  :4;
    unsigned cd     :1;
    unsigned ad     :1;
    unsigned unused :1;
    unsigned ra     :1;
    unsigned qdcount :16;
    unsigned ancount :16;
    unsigned nscount :16;
    unsigned arcount :16;
} HEADER;

/* Opcodes */
#define QUERY   0
#define IQUERY  1
#define STATUS  2

#define NS_QFIXEDSZ   4
#define NS_RRFIXEDSZ  10
#define NS_HFIXEDSZ   12
#define NS_MAXDNAME    1025
#define NS_MAXLABEL    63
#define NS_PACKETSZ    512
#define NS_MAXCDNAME   255

/* NS opcodes */
#define NS_O_QUERY  0
#define NS_O_IQUERY 1
#define NS_O_STATUS 2

/* NS response codes */
#define NS_R_NOERROR    0
#define NS_R_FORMERR    1
#define NS_R_SERVFAIL   2
#define NS_R_NXDOMAIN   3
#define NS_R_NOTIMPL    4
#define NS_R_REFUSED    5

/* NS classes */
#define NS_C_IN     1
#define NS_C_CHAOS  3
#define NS_C_ANY    255

/* NS types */
#define NS_T_A      1
#define NS_T_NS     2
#define NS_T_CNAME  5
#define NS_T_SOA    6
#define NS_T_PTR    12
#define NS_T_MX     15
#define NS_T_TXT    16
#define NS_T_AAAA   28
#define NS_T_SRV    33
#define NS_T_ANY    255

/* Section constants */
#define NS_S_QD  0
#define NS_S_AN  1
#define NS_S_NS  2
#define NS_S_AR  3

/* Message compression */
#define NS_CMPRSFLGS 0xC0

typedef enum __ns_sect {
    ns_s_qd  = 0,
    ns_s_an  = 1,
    ns_s_ns  = 2,
    ns_s_ar  = 3,
    ns_s_max = 4
} ns_sect;

typedef enum __ns_class {
    ns_c_in     = 1,
    ns_c_chaos  = 3,
    ns_c_any    = 255
} ns_class;

typedef enum __ns_type {
    ns_t_a      = 1,
    ns_t_ns     = 2,
    ns_t_cname  = 5,
    ns_t_soa    = 6,
    ns_t_ptr    = 12,
    ns_t_mx     = 15,
    ns_t_txt    = 16,
    ns_t_aaaa   = 28,
    ns_t_srv    = 33,
    ns_t_any    = 255
} ns_type;

typedef struct __ns_msg {
    const unsigned char *_msg;
    const unsigned char *_eom;
    uint16_t _id, _flags, _counts[ns_s_max];
    const unsigned char *_sections[ns_s_max];
    ns_sect _sect;
    int _rrnum;
    const unsigned char *_msg_ptr;
} ns_msg;

typedef struct __ns_rr {
    char name[NS_MAXDNAME];
    uint16_t type;
    uint16_t rr_class;
    uint32_t ttl;
    uint16_t rdlength;
    const unsigned char *rdata;
} ns_rr;

int ns_initparse(const unsigned char *msg, int msglen, ns_msg *handle);
int ns_parserr(ns_msg *handle, ns_sect section, int rrnum, ns_rr *rr);
int ns_name_uncompress(const unsigned char *msg, const unsigned char *eom,
                       const unsigned char *src, char *dst, size_t dstsiz);
int dn_expand(const unsigned char *msg, const unsigned char *eomorig,
              const unsigned char *comp_dn, char *exp_dn, int length);

/* Accessor macros */
#define ns_msg_id(handle)        ((handle)._id)
#define ns_msg_count(handle, s)  ((handle)._counts[(int)(s)])
#define ns_rr_name(rr)           ((rr).name)
#define ns_rr_type(rr)           ((rr).type)
#define ns_rr_class(rr)          ((rr).rr_class)
#define ns_rr_ttl(rr)            ((rr).ttl)
#define ns_rr_rdlen(rr)          ((rr).rdlength)
#define ns_rr_rdata(rr)          ((rr).rdata)

#endif /* _ARPA_NAMESER_H */
