#ifndef _LINUX_CAN_H
#define _LINUX_CAN_H

#include <stdint.h>

#define CAN_MAX_DLEN 8
#define CANFD_MAX_DLEN 64

typedef uint32_t canid_t;

struct can_frame {
    canid_t can_id;
    uint8_t can_dlc;
    uint8_t __pad;
    uint8_t __res0;
    uint8_t __res1;
    uint8_t data[CAN_MAX_DLEN];
};

struct canfd_frame {
    canid_t can_id;
    uint8_t len;
    uint8_t flags;
    uint8_t __res0;
    uint8_t __res1;
    uint8_t data[CANFD_MAX_DLEN];
};

#define CAN_RAW  1
#define CAN_BCM  2
#define CAN_J1939 7

#define SOL_CAN_BASE 100
#define SOL_CAN_RAW  (SOL_CAN_BASE + CAN_RAW)

#define CAN_RAW_FILTER          1
#define CAN_RAW_ERR_FILTER      2
#define CAN_RAW_LOOPBACK        3
#define CAN_RAW_RECV_OWN_MSGS   4
#define CAN_RAW_FD_FRAMES       5
#define CAN_RAW_JOIN_FILTERS    6

struct sockaddr_can {
    unsigned short can_family;
    int can_ifindex;
    union {
        struct { canid_t rx_id, tx_id; } tp;
        struct { canid_t name, addr; } j1939;
    } can_addr;
};

#endif /* _LINUX_CAN_H */
