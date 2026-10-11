/* Linked first of the machine's objects: where its static data starts (machine.h). One
 * initialised byte (.data) and one zeroed (.bss); the linker keeps input order. Mach-O puts
 * zeroed statics (__bss) apart from zeroed globals (__common): a static marker for those. */
char srally_state_data_begin[1] = { 1 };
char srally_state_bss_begin[1];
static char s_static_bss_begin[1];
char *srally_state_static_bss_begin(void) { return s_static_bss_begin; }
