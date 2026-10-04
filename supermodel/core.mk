# Compiles Supermodel's core plus our shim (build.sh sets things up and calls this).
#   make -f core.mk TARGET=web|headless|native SRC=<Supermodel checkout> GEN=<generated files> \
#        OBJ=<object dir> OUT=<dist dir> HERE=<supermodel/>
# web and headless share objects (TARGET=wasm); headless and native also link the no-op GL.
TARGET ?= web
SHIM := $(HERE)/shim

ifeq ($(TARGET),native)
  CXX := clang++
  CC := clang
  OBJDIR := $(OBJ)/native
  TARGET_FLAGS := -I$(GEN)/khronos
  EXCEPTIONS :=
  BIN := $(HERE)/.cache/native/bench
else
  CXX := em++
  CC := emcc
  OBJDIR := $(OBJ)/wasm
  TARGET_FLAGS := -sUSE_ZLIB=1
  EXCEPTIONS := -fwasm-exceptions
endif

INCLUDES := -I$(SHIM) -I$(SRC)/Src -I$(SRC)/Src/Model3 -I$(SRC)/Src/Model3/DriveBoard -I$(SRC)/Src/CPU \
  -I$(SRC)/Src/CPU/68K -I$(SRC)/Src/CPU/PowerPC -I$(SRC)/Src/CPU/Z80 -I$(SRC)/Src/Graphics \
  -I$(SRC)/Src/Graphics/New3D -I$(SRC)/Src/Inputs -I$(SRC)/Src/Network -I$(SRC)/Src/OSD \
  -I$(SRC)/Src/Sound -I$(SRC)/Src/Sound/MPEG -I$(SRC)/Src/Util -I$(GEN)
WARN := -Wall -Wno-unused-parameter -Wno-unused-variable -Wno-sign-compare -Wno-unused-but-set-variable \
  -Wno-deprecated-declarations -Wno-unused-function -Wno-missing-braces -Wno-char-subscripts
COMMON := -O3 -MMD -MP $(INCLUDES) $(TARGET_FLAGS) -DGLEW_STATIC $(EXCEPTIONS)
CXXFLAGS := $(COMMON) -std=c++17 $(WARN)
CFLAGS := $(COMMON) -std=gnu11 -w
MUSASHI_FLAGS := -DINLINE="static inline" -I$(SRC)/Src/CPU/68K/Musashi

# Supermodel's sources (Makefiles/Rules.inc) minus its SDL front end, legacy renderer, debugger
# and GLEW; the shim provides what those did.
SUPERMODEL_CPP := Src/BlockFile.cpp Src/GameLoader.cpp Src/ROMSet.cpp \
  Src/Model3/53C810.cpp Src/Model3/93C46.cpp Src/Model3/Crypto.cpp Src/Model3/DSB.cpp Src/Model3/IRQ.cpp \
  Src/Model3/JTAG.cpp Src/Model3/Model3.cpp Src/Model3/MPC10x.cpp Src/Model3/PCI.cpp Src/Model3/Real3D.cpp \
  Src/Model3/RTC72421.cpp Src/Model3/SoundBoard.cpp Src/Model3/TileGen.cpp \
  Src/Model3/DriveBoard/DriveBoard.cpp Src/Model3/DriveBoard/WheelBoard.cpp Src/Model3/DriveBoard/JoystickBoard.cpp \
  Src/Model3/DriveBoard/SkiBoard.cpp Src/Model3/DriveBoard/BillBoard.cpp Src/Model3/DriveBoard/Z80CTC.cpp \
  Src/CPU/PowerPC/ppc.cpp Src/CPU/PowerPC/PPCDisasm.cpp Src/CPU/68K/68K.cpp Src/CPU/Z80/Z80.cpp \
  Src/Sound/SCSP.cpp Src/Sound/SCSPDSP.cpp Src/Sound/MPEG/MpegAudio.cpp \
  Src/Graphics/Shader.cpp Src/Graphics/FBO.cpp Src/Graphics/Render2D.cpp Src/Graphics/SuperAA.cpp \
  Src/Graphics/New3D/GLSLShader.cpp Src/Graphics/New3D/R3DFrameBuffers.cpp Src/Graphics/New3D/New3D.cpp \
  Src/Graphics/New3D/Mat4.cpp Src/Graphics/New3D/Model.cpp Src/Graphics/New3D/PolyHeader.cpp \
  Src/Graphics/New3D/VBO.cpp Src/Graphics/New3D/Vec.cpp Src/Graphics/New3D/R3DShader.cpp \
  Src/Graphics/New3D/R3DFloat.cpp Src/Graphics/New3D/R3DScrollFog.cpp Src/Graphics/New3D/TextureBank.cpp \
  Src/Inputs/Input.cpp Src/Inputs/Inputs.cpp Src/Inputs/InputSource.cpp Src/Inputs/InputSystem.cpp \
  Src/Inputs/InputTypes.cpp Src/Inputs/MultiInputSource.cpp \
  Src/Network/NetBoard.cpp Src/Network/SimNetBoard.cpp \
  Src/OSD/Logger.cpp Src/OSD/Outputs.cpp \
  Src/Util/Format.cpp Src/Util/NewConfig.cpp Src/Util/ByteSwap.cpp Src/Util/ConfigBuilders.cpp \
  Src/Pkgs/tinyxml2.cpp
SUPERMODEL_C := Src/Pkgs/unzip.c Src/Pkgs/ioapi.c
MUSASHI_C := $(SRC)/Src/CPU/68K/Musashi/m68kcpu.c $(GEN)/m68kops.c $(GEN)/m68kopac.c $(GEN)/m68kopdm.c $(GEN)/m68kopnz.c
SHIM_CPP := libretro.cpp gl_shim.cpp RetroInputSystem.cpp osd/Audio.cpp osd/Thread.cpp osd/FileSystemPath.cpp \
  Network/TCPSend.cpp Network/TCPReceive.cpp Network/TCPSendAsync.cpp

obj_name = $(OBJDIR)/$(subst /,_,$(basename $(1))).o
CORE_OBJS := $(foreach s,$(SUPERMODEL_CPP) $(SUPERMODEL_C),$(call obj_name,$(s))) \
  $(foreach s,$(MUSASHI_C),$(OBJDIR)/musashi_$(basename $(notdir $(s))).o) \
  $(foreach s,$(SHIM_CPP),$(OBJDIR)/shim_$(subst /,_,$(basename $(s))).o)
NULL_GL_OBJ := $(OBJDIR)/gl_null.o

define CPP_RULE
$(call obj_name,$(1)): $(SRC)/$(1) | $(OBJDIR)
	$(CXX) $(CXXFLAGS) -c $$< -o $$@
endef
define C_RULE
$(call obj_name,$(1)): $(SRC)/$(1) | $(OBJDIR)
	$(CC) $(CFLAGS) -c $$< -o $$@
endef
define MUSASHI_RULE
$(OBJDIR)/musashi_$(basename $(notdir $(1))).o: $(1) $(GEN)/m68kops.h | $(OBJDIR)
	$(CC) $(CFLAGS) $(MUSASHI_FLAGS) -c $$< -o $$@
endef
define SHIM_RULE
$(OBJDIR)/shim_$(subst /,_,$(basename $(1))).o: $(SHIM)/$(1) | $(OBJDIR)
	$(CXX) $(CXXFLAGS) -c $$< -o $$@
endef
$(foreach s,$(SUPERMODEL_CPP),$(eval $(call CPP_RULE,$(s))))
$(foreach s,$(SUPERMODEL_C),$(eval $(call C_RULE,$(s))))
$(foreach s,$(MUSASHI_C),$(eval $(call MUSASHI_RULE,$(s))))
$(foreach s,$(SHIM_CPP),$(eval $(call SHIM_RULE,$(s))))

$(NULL_GL_OBJ): $(GEN)/gl_null.c | $(OBJDIR)
	$(CC) $(CFLAGS) -c $< -o $@

$(OBJDIR):
	mkdir -p $@

# Links: memory and stack sizes follow emulator/build.sh; a Model 3 set is up to ~250 MB of ROM.
# DEBUG=1 links with Emscripten's assertions (names in stack traces, uncaught exception text).
LINK_FLAGS := -O3 $(EXCEPTIONS) $(if $(DEBUG),-sASSERTIONS=1 -g2,) -sMODULARIZE=1 -sEXPORT_ES6=1 -sEXPORT_NAME=createSupermodel \
  -sINITIAL_MEMORY=268435456 -sALLOW_MEMORY_GROWTH=1 -sMAXIMUM_MEMORY=2147483648 -sSTACK_SIZE=4194304 \
  -sALLOW_TABLE_GROWTH=1 -sFORCE_FILESYSTEM=1 -sUSE_ZLIB=1 \
  -sEXPORTED_FUNCTIONS=@$(HERE)/exports.json \
  -sEXPORTED_RUNTIME_METHODS=FS,addFunction,removeFunction,UTF8ToString,stringToUTF8,lengthBytesUTF8,getValue,setValue,HEAPU8,HEAP16,HEAPU16,HEAP32,HEAPU32,specialHTMLTargets \
  --embed-file $(SRC)/Config/Games.xml@/Games.xml

.PHONY: web headless native
web: $(OUT)/supermodel.mjs
headless: $(OUT)/headless/supermodel.mjs
native: $(BIN)

$(OUT)/supermodel.mjs: $(CORE_OBJS)
	mkdir -p $(dir $@)
	em++ $^ $(LINK_FLAGS) -sENVIRONMENT=web,worker -sMAX_WEBGL_VERSION=2 -sMIN_WEBGL_VERSION=2 -o $@

$(OUT)/headless/supermodel.mjs: $(CORE_OBJS) $(NULL_GL_OBJ)
	mkdir -p $(dir $@)
	em++ $^ $(LINK_FLAGS) -sENVIRONMENT=node -o $@

$(BIN): $(CORE_OBJS) $(NULL_GL_OBJ) $(OBJDIR)/main_native.o
	mkdir -p $(dir $@)
	$(CXX) -O3 $^ -lz -o $@

$(OBJDIR)/main_native.o: $(SHIM)/main_native.cpp | $(OBJDIR)
	$(CXX) $(CXXFLAGS) -c $< -o $@

-include $(wildcard $(OBJDIR)/*.d)
