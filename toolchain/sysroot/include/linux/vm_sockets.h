#ifndef _LINUX_VM_SOCKETS_H
#define _LINUX_VM_SOCKETS_H

#define VMADDR_CID_ANY         -1U
#define VMADDR_CID_HYPERVISOR  0
#define VMADDR_CID_LOCAL       1
#define VMADDR_CID_HOST        2
#define VMADDR_PORT_ANY        -1U

struct sockaddr_vm {
    unsigned short svm_family;
    unsigned short svm_reserved1;
    unsigned int   svm_port;
    unsigned int   svm_cid;
    unsigned char  svm_zero[sizeof(struct sockaddr) - sizeof(unsigned short) - sizeof(unsigned short) - sizeof(unsigned int) - sizeof(unsigned int)];
};

#endif /* _LINUX_VM_SOCKETS_H */
