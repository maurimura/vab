# Checks a wasm-ld link map (core.mk links with --Map): everything between the machine's markers
# (shim/state_begin.c, state_end.c), in .data and in .bss, must be the machine's own objects
# (the recomp's and shim/machine.c), never the C library's or the shim's other files, since a
# save state copies those ranges wholesale (shim/machine.h). Prints a summary; exit 1 if not.
#   awk -v objdir=<the wasm object directory> -f layout.awk srally.map
/\(\.data\.srally_state_data_begin\)/ { data = 1 }
/\(\.bss\.(s_static_bss_begin|srally_state_bss_begin)\)/ { bss = 1 }
(data || bss) && /:\(/ {
  if (data) nd++; else nb++
  if (index($0, objdir "/") == 0 || $0 ~ /\/(shim_(arena|coro|zip|libretro|video|video_stub|main_native)[^\/]*|render_[^\/]*)\.o:/) {
    bad = bad "\n  " $0
  }
}
/\(\.data\.srally_state_data_end\)/ { data = 0 }
/\(\.bss\.(s_static_bss_end|srally_state_bss_end)\)/ { if (bss && $0 ~ /srally_state_bss_end/) bss = 0 }
END {
  if (nd == 0 || nb == 0) { print "layout: the markers are missing from the map"; exit 1 }
  if (bad != "") { print "layout: not the machine's, inside its ranges:" bad; exit 1 }
  print "layout: the machine's ranges hold " nd " initialised and " nb " zeroed objects, all its own"
}
