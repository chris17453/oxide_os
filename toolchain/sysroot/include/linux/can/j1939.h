#ifndef _LINUX_CAN_J1939_H
#define _LINUX_CAN_J1939_H

#include <linux/can.h>

typedef unsigned long long name_t;
typedef unsigned int pgn_t;

#define J1939_MAX_UNICAST_ADDR 0xfd
#define J1939_IDLE_ADDR        0xfe
#define J1939_NO_ADDR          0xff
#define J1939_NO_NAME          0
#define J1939_PGN_REQUEST      0x0ea00
#define J1939_PGN_ADDRESS_CLAIMED 0x0ee00
#define J1939_PGN_PDU1_MAX     0x3ff00
#define J1939_PGN_MAX          0x3ffff

#define SOL_CAN_J1939 (SOL_CAN_BASE + CAN_J1939)

#define SO_J1939_FILTER          1
#define SO_J1939_PROMISC         2
#define SO_J1939_SEND_PRIO       3
#define SO_J1939_ERRQUEUE        4

#endif /* _LINUX_CAN_J1939_H */
