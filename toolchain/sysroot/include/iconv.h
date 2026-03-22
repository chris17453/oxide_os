/* OXIDE OS iconv — character set conversion
 * — PulseForge: Minimal iconv for glib's UTF-8 handling. glib only uses
 * iconv for encoding conversion between UTF-8 and locale charsets.
 * Since OXIDE is UTF-8 everywhere, most conversions are identity transforms.
 */

#ifndef _ICONV_H
#define _ICONV_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef void *iconv_t;

iconv_t iconv_open(const char *tocode, const char *fromcode);
size_t iconv(iconv_t cd, char **inbuf, size_t *inbytesleft,
             char **outbuf, size_t *outbytesleft);
int iconv_close(iconv_t cd);

#ifdef __cplusplus
}
#endif

#endif /* _ICONV_H */
