// Linked into the outrun core only (emulator/build.sh). That core keeps Sega's Out Run board and
// the System 16 code it shares with Sega's other boards, but not those boards' drivers, to stay
// small. sys16_run.cpp still points the CPUs at every board's handlers, picking them by board
// at init, so those it never picks for Out Run are these stand-ins, never called. Declared as
// in FBNeo's src/burn/drv/sega/sys16.h (burn.h's types; __fastcall is empty off x86).
typedef unsigned char UINT8;
typedef unsigned short UINT16;
typedef unsigned int UINT32;
typedef signed int INT32;

// d_sys16a.cpp
void System16APPI0WritePortA(UINT8) {}
void System16APPI0WritePortB(UINT8) {}
void System16APPI0WritePortC(UINT8) {}
UINT16 System16AReadWord(UINT32) { return 0xffff; }
UINT8 System16AReadByte(UINT32) { return 0xff; }
void System16AWriteWord(UINT32, UINT16) {}
void System16AWriteByte(UINT32, UINT8) {}
UINT8 System16A_I8751ReadPort(INT32) { return 0xff; }
void System16A_I8751WritePort(INT32, UINT8) {}

// d_sys18.cpp
UINT8 system18_io_chip_r(UINT32) { return 0xff; }
void system18_io_chip_w(UINT32, UINT16) {}
void System18GfxBankWrite(UINT32, UINT16) {}
void HamawayGfxBankWrite(UINT32, UINT16) {}

// d_hangon.cpp
void HangonPPI0WritePortA(UINT8) {}
void HangonPPI0WritePortB(UINT8) {}
void HangonPPI0WritePortC(UINT8) {}
UINT8 HangonPPI1ReadPortC() { return 0xff; }
void HangonPPI1WritePortA(UINT8) {}
UINT16 HangonReadWord(UINT32) { return 0xffff; }
UINT8 HangonReadByte(UINT32) { return 0xff; }
void HangonWriteWord(UINT32, UINT16) {}
void HangonWriteByte(UINT32, UINT8) {}
UINT8 Hangon_I8751ReadPort(INT32) { return 0xff; }
void Hangon_I8751WritePort(INT32, UINT8) {}

// d_xbrd.cpp
UINT16 XBoardReadWord(UINT32) { return 0xffff; }
UINT8 XBoardReadByte(UINT32) { return 0xff; }
void XBoardWriteWord(UINT32, UINT16) {}
void XBoardWriteByte(UINT32, UINT8) {}
UINT16 XBoard2ReadWord(UINT32) { return 0xffff; }
UINT8 XBoard2ReadByte(UINT32) { return 0xff; }
void XBoard2WriteWord(UINT32, UINT16) {}
void XBoard2WriteByte(UINT32, UINT8) {}

// d_ybrd.cpp
UINT16 YBoardReadWord(UINT32) { return 0xffff; }
UINT8 YBoardReadByte(UINT32) { return 0xff; }
void YBoardWriteWord(UINT32, UINT16) {}
void YBoardWriteByte(UINT32, UINT8) {}
UINT16 YBoard2ReadWord(UINT32) { return 0xffff; }
void YBoard2WriteWord(UINT32, UINT16) {}
UINT16 YBoard3ReadWord(UINT32) { return 0xffff; }
UINT8 YBoard3ReadByte(UINT32) { return 0xff; }
void YBoard3WriteWord(UINT32, UINT16) {}
