# Compiles the recomp's runtime, SoftFloat, ymfm, the game code and our shim (build.sh sets
# things up and calls this).
#   make -f core.mk TARGET=web|headless|native SRC=<recomp checkout> GEN=<generated code> \
#        GEN_MODE=rom|stub OBJ=<object dir> OUT=<dist dir> HERE=<daytona/> SOFTFLOAT_MK=<file list>
# web and headless share objects (wasm); native is the same code with clang, for bench.
TARGET ?= web
GEN_MODE ?= stub
NATIVE_CC ?= clang
NATIVE_CXX ?= clang++
SHIM := $(HERE)/shim
include $(SOFTFLOAT_MK)

ifeq ($(TARGET),native)
  CXX := $(NATIVE_CXX)
  CC := $(NATIVE_CC)
  OBJDIR := $(OBJ)/native
  EXCEPTIONS :=
  BIN := $(HERE)/.cache/native/bench
else
  CXX := em++
  CC := emcc
  OBJDIR := $(OBJ)/wasm
  EXCEPTIONS ?= -fwasm-exceptions
endif

SF := $(SRC)/extern/softfloat
YMFM := $(SRC)/extern/ymfm/src

# Never fast-math, and no FP contraction: exact rounding is the point (the recomp's rules.md; a
# fused multiply-add rounds once where the game's code rounds twice).
FP := -fno-fast-math -ffp-contract=off
# PERF_FLAGS: extra codegen flags to try (e.g. "-flto -msimd128"), on compile and link.
PERF_FLAGS ?=
OPT ?= -O3
COMMON := $(OPT) $(FP) -DNDEBUG -MMD -MP $(EXCEPTIONS) $(PERF_FLAGS)
# SoftFloat 3e as the recomp builds it: FAST_INT64 API, little-endian, 8086-SSE specialisation,
# GCC platform header (__int128 and builtins, which clang has for wasm32 too). Its rounding mode
# and flags are plain globals: there are no threads here.
SF_PUBLIC := -I$(SF)/source/include -DSOFTFLOAT_FAST_INT64 -DLITTLEENDIAN=1 -DTHREAD_LOCAL=
SF_PRIVATE := -I$(SF)/build/Linux-x86_64-GCC -I$(SF)/source/8086-SSE \
  -DSOFTFLOAT_ROUND_ODD -DINLINE_LEVEL=5 -DSOFTFLOAT_FAST_DIV32TO16 -DSOFTFLOAT_FAST_DIV64TO32
RECOMP_FLAGS := $(COMMON) -std=c++20 -I$(SRC)/src -isystem $(YMFM) $(SF_PUBLIC) -DM2_ROMSET=\"daytona\"
CXXFLAGS := $(RECOMP_FLAGS) -w
SHIM_CXXFLAGS := $(RECOMP_FLAGS) -I$(SHIM) -I$(SRC)/tools -I$(OBJ) -Wall -Wextra -Wno-unused-parameter
CFLAGS := $(COMMON) -std=gnu11 $(SF_PUBLIC) $(SF_PRIVATE) -w

# The recomp's runtime library (CMakeLists.txt: runtime, i960, trace) minus nothing; 7z ROM sets
# are left out (no LZMA SDK: zip only). snapshot.cpp comes with patches/0001-snapshot.patch.
RUNTIME_CPP := src/runtime/cpu.cpp src/runtime/tgp.cpp src/runtime/m2_tgp_board.cpp src/runtime/geo.cpp \
  src/runtime/raster.cpp src/runtime/video.cpp src/runtime/m2_board.cpp src/runtime/game_loop.cpp \
  src/runtime/zip.cpp src/runtime/rom_import.cpp src/runtime/archive.cpp src/runtime/m2_replay_bus.cpp \
  src/runtime/lockstep.cpp src/runtime/snd_cpu.cpp src/runtime/multipcm.cpp src/runtime/sound_board.cpp \
  src/runtime/native_sample_mixer.cpp src/runtime/native_sound_sequencer.cpp src/runtime/native_sound_engine.cpp \
  src/runtime/enhance.cpp src/runtime/comm_board.cpp \
  src/i960/isa.cpp src/i960/decode.cpp src/i960/reach.cpp src/i960/fp.cpp src/trace/trace.cpp
SNAPSHOT_CPP := $(wildcard $(SRC)/src/runtime/snapshot.cpp)
ifeq ($(SNAPSHOT_CPP),)
  # Until the runtime's save states land: shim/snapshot_stub.cpp (no states; RAM copied out).
  SNAP := nosnap
  SNAP_DEFS := -DDAYTONA_NO_SNAPSHOT
else
  RUNTIME_CPP += src/runtime/snapshot.cpp
  SNAP := snap
  SNAP_DEFS :=
endif
YMFM_CPP := ymfm_opn.cpp ymfm_adpcm.cpp ymfm_ssg.cpp

# The game code: generated from the ROM set (build.sh), or the stub that only links.
ifeq ($(GEN_MODE),rom)
  GEN_CPP := $(wildcard $(GEN)/daytona/*.cpp) $(GEN)/daytona_tgp/tgp_gen.cpp $(GEN)/daytona_snd/snd_gen.cpp
else
  GEN_CPP := $(SHIM)/gen_stub.cpp
endif
GEN_OBJDIR := $(OBJDIR)/gen-$(GEN_MODE)
SHIM_CPP := libretro.cpp snapshot_stub.cpp

obj_name = $(OBJDIR)/$(subst /,_,$(basename $(1))).o
RUNTIME_OBJS := $(foreach s,$(RUNTIME_CPP),$(call obj_name,$(s)))
SF_OBJS := $(foreach s,$(SOFTFLOAT_C),$(OBJDIR)/sf_$(subst /,_,$(basename $(s))).o)
YMFM_OBJS := $(foreach s,$(YMFM_CPP),$(OBJDIR)/ymfm_$(basename $(s)).o)
GEN_OBJS := $(foreach s,$(GEN_CPP),$(GEN_OBJDIR)/$(notdir $(basename $(s))).o)
SHIM_OBJS := $(foreach s,$(SHIM_CPP),$(OBJDIR)/shim_$(basename $(s))_$(SNAP).o)
CORE_OBJS := $(RUNTIME_OBJS) $(SF_OBJS) $(YMFM_OBJS) $(GEN_OBJS) $(SHIM_OBJS)

# A link input list that changes when the set of objects does (stub <-> ROM game code, the
# snapshot patch), so the outputs relink even when every object is older than them.
$(shell mkdir -p $(OBJDIR); echo "$(CORE_OBJS)" | cmp -s - $(OBJDIR)/link.list || echo "$(CORE_OBJS)" > $(OBJDIR)/link.list)

define RUNTIME_RULE
$(call obj_name,$(1)): $(SRC)/$(1) | $(OBJDIR)
	$(CXX) $(CXXFLAGS) -c $$< -o $$@
endef
define SF_RULE
$(OBJDIR)/sf_$(subst /,_,$(basename $(1))).o: $(SF)/$(1) | $(OBJDIR)
	$(CC) $(CFLAGS) -c $$< -o $$@
endef
define YMFM_RULE
$(OBJDIR)/ymfm_$(basename $(1)).o: $(YMFM)/$(1) | $(OBJDIR)
	$(CXX) $(CXXFLAGS) -c $$< -o $$@
endef
define GEN_RULE
$(GEN_OBJDIR)/$(notdir $(basename $(1))).o: $(1) | $(GEN_OBJDIR)
	$(CXX) $(CXXFLAGS) -c $$< -o $$@
endef
define SHIM_RULE
$(OBJDIR)/shim_$(basename $(1))_$(SNAP).o: $(SHIM)/$(1) | $(OBJDIR)
	$(CXX) $(SHIM_CXXFLAGS) $(SNAP_DEFS) -c $$< -o $$@
endef
$(foreach s,$(RUNTIME_CPP),$(eval $(call RUNTIME_RULE,$(s))))
$(foreach s,$(SOFTFLOAT_C),$(eval $(call SF_RULE,$(s))))
$(foreach s,$(YMFM_CPP),$(eval $(call YMFM_RULE,$(s))))
$(foreach s,$(GEN_CPP),$(eval $(call GEN_RULE,$(s))))
$(foreach s,$(SHIM_CPP),$(eval $(call SHIM_RULE,$(s))))

$(OBJDIR) $(GEN_OBJDIR):
	mkdir -p $@

# Presets for the cabinets' settings, when daytona/nvram/<cabinets>/<cabinet>/ hold them
# (make-nvram.mjs): embedded at /daytona/nvram, where the shim looks after /nvram/<cabinet>/.
PRESETS := $(wildcard $(HERE)/nvram/1 $(HERE)/nvram/2)
EMBED := $(foreach p,$(PRESETS),--embed-file $(p)@/daytona/nvram/$(notdir $(p)))

# Links: memory and stack as supermodel/core.mk; each cabinet holds its own ~75 MB of ROM images.
# DEBUG=1 links with Emscripten's assertions (names in stack traces, uncaught exception text).
LINK_FLAGS := -O3 $(EXCEPTIONS) $(PERF_FLAGS) $(if $(DEBUG),-sASSERTIONS=1 -g2,) -sMODULARIZE=1 -sEXPORT_ES6=1 -sEXPORT_NAME=createDaytona \
  -sINITIAL_MEMORY=268435456 -sALLOW_MEMORY_GROWTH=1 -sMAXIMUM_MEMORY=2147483648 -sSTACK_SIZE=4194304 \
  -sALLOW_TABLE_GROWTH=1 -sFORCE_FILESYSTEM=1 \
  -sEXPORTED_FUNCTIONS=@$(HERE)/exports.json \
  -sEXPORTED_RUNTIME_METHODS=FS,addFunction,removeFunction,UTF8ToString,stringToUTF8,lengthBytesUTF8,getValue,setValue,HEAPU8,HEAP16,HEAPU16,HEAP32,HEAPU32 \
  $(EMBED)

.PHONY: web headless native
web: $(OUT)/daytona.mjs
headless: $(OUT)/headless/daytona.mjs
native: $(BIN)

$(OUT)/daytona.mjs: $(CORE_OBJS) $(OBJDIR)/link.list $(HERE)/exports.json $(PRESETS)
	mkdir -p $(dir $@)
	em++ $(CORE_OBJS) $(LINK_FLAGS) -sENVIRONMENT=web,worker -o $@

$(OUT)/headless/daytona.mjs: $(CORE_OBJS) $(OBJDIR)/link.list $(HERE)/exports.json $(PRESETS)
	mkdir -p $(dir $@)
	em++ $(CORE_OBJS) $(LINK_FLAGS) -sENVIRONMENT=node -o $@

$(BIN): $(CORE_OBJS) $(OBJDIR)/link.list $(OBJDIR)/main_native.o
	mkdir -p $(dir $@)
	$(CXX) -O3 $(CORE_OBJS) $(OBJDIR)/main_native.o -o $@

$(OBJDIR)/main_native.o: $(SHIM)/main_native.cpp | $(OBJDIR)
	$(CXX) $(SHIM_CXXFLAGS) -c $< -o $@

-include $(wildcard $(OBJDIR)/*.d $(GEN_OBJDIR)/*.d)
