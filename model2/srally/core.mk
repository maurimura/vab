# Compiles segarally95-recomp's runtime and lifted game with our shim (build.sh sets things up
# and calls this).
#   make -f core.mk web|headless|native SRC=<recomp checkout> OBJ=<object dir> OUT=<dist dir> HERE=<model2/srally>
# web and headless share objects (wasm) but the renderer's; native is the same code with clang,
# for the bench.
TARGET ?= web
NATIVE_CC ?= clang
SHIM := $(HERE)/shim

ifeq ($(TARGET),native)
  CC := $(NATIVE_CC)
  OBJDIR := $(OBJ)/native
  BIN := $(HERE)/.cache/native/bench
else
  CC := emcc
  OBJDIR := $(OBJ)/wasm
endif

# Never fast-math, and no FP contraction: the lifted code's rounding is the i960's, a fused
# multiply-add rounds once where the game rounds twice (i960_fp.h).
FP := -fno-fast-math -ffp-contract=off
# PERF_FLAGS: extra codegen flags to try, on compile and link.
PERF_FLAGS ?=
OPT ?= -O3
COMMON := $(OPT) $(FP) -DNDEBUG -MMD -MP $(PERF_FLAGS)
INCLUDES := -I$(SRC)/src -I$(SRC)/include -I$(SRC)/lib/model2/include -I$(SRC)/lib/model2/host \
  -I$(SRC)/lib/model2/geo/include -I$(SRC)/lib/model2/hw/include -I$(SRC)/lib/model2/tgp/include \
  -I$(SRC)/lib/model2/snd/include -I$(SRC)/lib/host_compat
# The recomp as its CMake builds it without SDL2, libpng and OpenGL (the viewer's window, PNG
# dumps and GL composite compiled out), its heap in our arenas (shim/srally_alloc.h).
RECOMP_CFLAGS := $(COMMON) -std=gnu99 $(INCLUDES) -DHOST_COMPAT_HAVE_CLOCK_GETTIME -include $(SHIM)/srally_alloc.h -w
SHIM_CFLAGS := $(COMMON) -std=gnu11 $(INCLUDES) -I$(SHIM) -I$(OBJ) -Wall -Wextra -Wno-unused-parameter -Wno-missing-field-initializers
ifeq ($(TARGET),native)
  SHIM_CFLAGS += -Wno-deprecated-declarations
endif

# Every source of the CMake target segamod2 but its main (lift_main.c), in two parts: the
# machine (the game, its board, the sound board, the geometry decode: in a save state) and the
# renderer's (GL draw, tile layers, the SDL viewer's file: drawing only, linked outside the
# machine, its heap the C library's).
RENDER_C := $(SRC)/lib/model2/geo/model2_geo_gl.c $(SRC)/lib/host_compat/model2_gl.c $(SRC)/src/host/sys24_tile.c \
  $(SRC)/src/host/sys24_gfx.c $(SRC)/src/host/sys24_viewer.c $(SRC)/src/host/sys24_viewer_record.c \
  $(SRC)/src/host/sys24_png_write.c
RECOMP_C := $(filter-out $(RENDER_C),$(sort $(wildcard $(SRC)/lib/model2/geo/*.c $(SRC)/lib/model2/hw/*.c \
  $(SRC)/lib/model2/tgp/*.c $(SRC)/lib/model2/snd/*.c $(SRC)/lib/model2/host/*.c $(SRC)/src/game/*.c \
  $(SRC)/src/host/*.c $(SRC)/src/libc/*.c $(SRC)/src/boot/*.c $(SRC)/src/irq/*.c))) \
  $(SRC)/src/i960_host_syms.c $(SRC)/src/i960_host_invoke.c

# The renderer: video.c when its port is here, else the black stand-in. With GL in the web
# build only (RENDER_GL_FLAGS; Node and the native bench draw nothing).
# video.c draws with the renderer's patch to model2_geo_gl.c (offscreen 3D); until that patch is
# in patches/ (and applied), the stand-in draws the tile layers alone on the CPU.
RENDER_GL_READY := $(if $(wildcard $(SHIM)/video.c),$(shell grep -l model2_geo_gl_render_offscreen $(SRC)/lib/model2/geo/model2_geo_gl.c 2>/dev/null),)
VIDEO_C := $(if $(RENDER_GL_READY),video.c,video_stub.c)
RENDER_GL_FLAGS ?= $(if $(RENDER_GL_READY),-DI960_HOST_HAVE_GL -DSRALLY_HAVE_GL,)
RENDER_CFLAGS := $(COMMON) -std=gnu99 $(INCLUDES) -I$(SHIM) -DHOST_COMPAT_HAVE_CLOCK_GETTIME -w $(if $(filter web,$(TARGET)),$(RENDER_GL_FLAGS),)
# The machine's objects lie between state_begin and state_end (machine.h); the shim's own
# globals come after, outside a save state.
SHIM_C := arena.c coro.c zip.c

obj_name = $(OBJDIR)/$(subst /,_,$(patsubst $(SRC)/%,%,$(basename $(1)))).o
render_obj = $(OBJDIR)/render_$(subst /,_,$(patsubst $(SRC)/%,%,$(basename $(1))))_$(TARGET).o
RECOMP_OBJS := $(foreach s,$(RECOMP_C),$(call obj_name,$(s)))
RENDER_OBJS := $(foreach s,$(RENDER_C),$(call render_obj,$(s)))
SHIM_OBJS := $(foreach s,$(SHIM_C),$(OBJDIR)/shim_$(basename $(s)).o) $(OBJDIR)/shim_libretro_$(TARGET).o
VIDEO_OBJ := $(OBJDIR)/shim_$(basename $(VIDEO_C))_$(TARGET).o
CORE_OBJS := $(OBJDIR)/shim_state_begin.o $(RECOMP_OBJS) $(OBJDIR)/shim_machine.o $(OBJDIR)/shim_state_end.o \
  $(SHIM_OBJS) $(VIDEO_OBJ) $(RENDER_OBJS)

define RENDER_RULE
$(call render_obj,$(1)): $(1) | $(OBJDIR)
	$$(CC) $$(RENDER_CFLAGS) -c $$< -o $$@
endef
$(foreach s,$(RENDER_C),$(eval $(call RENDER_RULE,$(s))))

define RECOMP_RULE
$(call obj_name,$(1)): $(1) $(SHIM)/srally_alloc.h | $(OBJDIR)
	$(CC) $(RECOMP_CFLAGS) -c $$< -o $$@
endef
$(foreach s,$(RECOMP_C),$(eval $(call RECOMP_RULE,$(s))))

$(OBJDIR)/shim_%.o: $(SHIM)/%.c | $(OBJDIR)
	$(CC) $(SHIM_CFLAGS) -c $< -o $@

# libretro.c includes video.h when video.c is the renderer.
$(OBJDIR)/shim_libretro_$(TARGET).o: $(SHIM)/libretro.c | $(OBJDIR)
	$(CC) $(SHIM_CFLAGS) $(if $(filter video.c,$(VIDEO_C)),-DSRALLY_VIDEO_C,) -c $< -o $@

$(OBJDIR)/shim_$(basename $(VIDEO_C))_$(TARGET).o: $(SHIM)/$(VIDEO_C) | $(OBJDIR)
	$(CC) $(RENDER_CFLAGS) -c $< -o $@

$(OBJDIR):
	mkdir -p $@

# Links: memory as daytona/ and supermodel/ (256 MB initial, growth to 2 GB, 4 MB stack);
# Asyncify for the game's coroutine (coro.c): every indirect call stays instrumented (the
# lifted code dispatches through i960_call_indirect), the stack it unwinds into is the
# machine's (SRALLY_ASYNCIFY_STACK), not ASYNCIFY_STACK_SIZE.
# ASYNCIFY_FLAGS: extra Asyncify settings to try (e.g. ASYNCIFY_REMOVE lists).
ASYNCIFY_FLAGS ?=
LINK_FLAGS := -O3 $(PERF_FLAGS) $(if $(DEBUG),-sASSERTIONS=1 -g2,) -sMODULARIZE=1 -sEXPORT_ES6=1 -sEXPORT_NAME=createSrally \
  -sINITIAL_MEMORY=268435456 -sALLOW_MEMORY_GROWTH=1 -sMAXIMUM_MEMORY=2147483648 -sSTACK_SIZE=4194304 \
  -sALLOW_TABLE_GROWTH=1 -sFORCE_FILESYSTEM=1 -sUSE_ZLIB=1 \
  -sASYNCIFY=1 -sASYNCIFY_STACK_SIZE=65536 $(ASYNCIFY_FLAGS) \
  -sEXPORTED_FUNCTIONS=@$(HERE)/exports.json \
  -sEXPORTED_RUNTIME_METHODS=FS,addFunction,removeFunction,UTF8ToString,stringToUTF8,lengthBytesUTF8,getValue,setValue,HEAPU8,HEAP16,HEAPU16,HEAP32,HEAPU32,specialHTMLTargets

.PHONY: web headless native
web: $(OUT)/srally.mjs
headless: $(OUT)/headless/srally.mjs
native: $(BIN)

# The renderer's extra link flags, from model2/srally/video-notes.md's port (VIDEO_LDFLAGS).
# Each link writes its map and layout.awk checks the machine's ranges in it (a save state
# copies them: shim/machine.h).
$(OUT)/srally.mjs: $(CORE_OBJS) $(HERE)/exports.json $(HERE)/layout.awk
	mkdir -p $(dir $@)
	emcc $(CORE_OBJS) $(LINK_FLAGS) -sENVIRONMENT=web,worker -sMAX_WEBGL_VERSION=2 -sMIN_WEBGL_VERSION=2 $(VIDEO_LDFLAGS) \
	  -Wl,--Map=$(OBJDIR)/srally-web.map -o $@
	awk -v objdir=$(OBJDIR) -f $(HERE)/layout.awk $(OBJDIR)/srally-web.map || { rm -f $@; exit 1; }

$(OUT)/headless/srally.mjs: $(CORE_OBJS) $(HEADLESS_EXTRA) $(HERE)/exports.json $(HERE)/layout.awk
	mkdir -p $(dir $@)
	emcc $(CORE_OBJS) $(HEADLESS_EXTRA) $(LINK_FLAGS) -sENVIRONMENT=node -Wl,--Map=$(OBJDIR)/srally-headless.map -o $@
	awk -v objdir=$(OBJDIR) -f $(HERE)/layout.awk $(OBJDIR)/srally-headless.map || { rm -f $@; exit 1; }

$(BIN): $(CORE_OBJS) $(OBJDIR)/shim_main_native.o
	mkdir -p $(dir $@)
	$(CC) -O3 $^ -lz -o $@

-include $(wildcard $(OBJDIR)/*.d)
