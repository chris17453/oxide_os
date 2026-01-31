#ifndef _READLINE_HISTORY_H
#define _READLINE_HISTORY_H

#ifdef __cplusplus
extern "C" {
#endif

typedef void *histdata_t;

typedef struct _hist_entry {
    char *line;
    char *timestamp;
    void *data;
} HIST_ENTRY;

typedef struct _hist_state {
    HIST_ENTRY **entries;
    int offset;
    int length;
    int size;
    int flags;
} HISTORY_STATE;

/* History management */
void add_history(const char *line);
void using_history(void);
void clear_history(void);
HIST_ENTRY *history_get(int offset);
HIST_ENTRY *remove_history(int which);
HIST_ENTRY *replace_history_entry(int which, const char *line, histdata_t data);
histdata_t free_history_entry(HIST_ENTRY *entry);
HISTORY_STATE *history_get_history_state(void);

/* History file I/O */
int read_history(const char *filename);
int write_history(const char *filename);
int append_history(int nelements, const char *filename);
int history_truncate_file(const char *filename, int nlines);

/* Global */
extern int history_length;

#ifdef __cplusplus
}
#endif

#endif /* _READLINE_HISTORY_H */
