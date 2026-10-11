/* Linked last of the machine's objects: where its static data ends (machine.h). */
char srally_state_data_end[1] = { 1 };
char srally_state_bss_end[1];
static char s_static_bss_end[1];
char *srally_state_static_bss_end(void) { return s_static_bss_end; }
