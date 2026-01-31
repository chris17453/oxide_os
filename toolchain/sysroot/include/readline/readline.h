#ifndef _READLINE_READLINE_H
#define _READLINE_READLINE_H

#include <stdio.h>
#include <readline/history.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Type definitions */
typedef void *Keymap;
typedef int rl_command_func_t(int, int);
typedef char *rl_compentry_func_t(const char *, int);
typedef char **rl_completion_func_t(const char *, int, int);
typedef void rl_compdisp_func_t(char **, int, int);
typedef int rl_hook_func_t(void);
typedef void rl_vcpfunc_t(char *);
typedef rl_compentry_func_t CPFunction;
typedef rl_completion_func_t CPPFunction;

/* Core functions */
char *readline(const char *prompt);
int rl_initialize(void);
void rl_callback_handler_install(const char *prompt, rl_vcpfunc_t *handler);
void rl_callback_read_char(void);
void rl_callback_handler_remove(void);
void rl_callback_sigcleanup(void);

/* Line manipulation */
int rl_insert_text(const char *text);
void rl_redisplay(void);
int rl_insert(int count, int key);
int rl_complete(int count, int key);
void rl_prep_terminal(int meta_flag);
void rl_resize_terminal(void);
void rl_free_line_state(void);
void rl_cleanup_after_signal(void);

/* Key binding */
int rl_bind_key(int key, rl_command_func_t *function);
int rl_bind_key_in_map(int key, rl_command_func_t *function, Keymap map);
int rl_parse_and_bind(char *line);
int rl_read_init_file(const char *filename);
int rl_variable_bind(const char *variable, const char *value);

/* Completion */
char **completion_matches(char *text, rl_compentry_func_t *func);
char **rl_completion_matches(char *text, rl_compentry_func_t *func);

/* Global variables */
extern char *rl_readline_name;
extern char *rl_line_buffer;
extern int rl_point;
extern int rl_end;
extern FILE *rl_instream;
extern FILE *rl_outstream;
extern const char *rl_library_version;
extern int rl_readline_version;
extern int rl_catch_signals;
extern rl_completion_func_t *rl_attempted_completion_function;
extern int rl_attempted_completion_over;
extern const char *rl_completer_word_break_characters;
extern const char *rl_basic_word_break_characters;
extern int rl_completion_append_character;
extern int rl_completion_suppress_append;
extern rl_compdisp_func_t *rl_completion_display_matches_hook;
extern int rl_completion_type;
extern rl_hook_func_t *rl_startup_hook;
extern rl_hook_func_t *rl_pre_input_hook;
extern Keymap emacs_meta_keymap;
extern int history_length;

/* Version macros */
#define RL_READLINE_VERSION 0x0800
#define RL_VERSION_MAJOR    8
#define RL_VERSION_MINOR    0

#ifdef __cplusplus
}
#endif

#endif /* _READLINE_READLINE_H */
