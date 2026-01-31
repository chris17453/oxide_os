#ifndef _CRYPT_H
#define _CRYPT_H

char *crypt(const char *key, const char *salt);
char *crypt_r(const char *key, const char *salt, void *data);

#endif /* _CRYPT_H */
