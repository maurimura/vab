// Stand-in for Supermodel's OSD/SDL/SDLIncludes.h, which the network board code includes for
// SDL_net. This build has no sockets: the Model 3 network board is either simulated
// (SimulateNet) or unused, so its TCP classes are stubs (Network/*.cpp here).
#pragma once

typedef void *TCPsocket;
typedef void *SDLNet_SocketSet;

#define SDL_MESSAGEBOX_ERROR 0
static inline int SDL_ShowSimpleMessageBox(unsigned flags, const char *title, const char *message, void *window)
{
  (void)flags; (void)title; (void)message; (void)window;
  return 0;
}
