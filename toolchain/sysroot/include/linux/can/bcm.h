#ifndef _LINUX_CAN_BCM_H
#define _LINUX_CAN_BCM_H

#include <linux/can.h>

#define TX_SETUP    1
#define TX_DELETE   2
#define TX_READ     3
#define TX_SEND     4
#define RX_SETUP    5
#define RX_DELETE   6
#define RX_READ     7
#define TX_STATUS   8
#define TX_EXPIRED  9
#define RX_STATUS   10
#define RX_TIMEOUT  11
#define RX_CHANGED  12

struct bcm_msg_head {
    uint32_t opcode;
    uint32_t flags;
    uint32_t count;
    struct timeval ival1, ival2;
    canid_t can_id;
    uint32_t nframes;
    struct can_frame frames[0];
};

#endif /* _LINUX_CAN_BCM_H */
