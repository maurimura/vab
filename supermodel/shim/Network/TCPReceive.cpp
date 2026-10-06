// No sockets in this build (see SDLIncludes.h): the network board never receives.
#include "Network/TCPReceive.h"

TCPReceive::TCPReceive(int port) : m_listenSocket(nullptr), m_receiveSocket(nullptr), m_socketSet(nullptr), m_running(false)
{
  (void)port;
}
TCPReceive::~TCPReceive() {}
bool TCPReceive::CheckDataAvailable(int timeoutMS) { (void)timeoutMS; return false; }
std::vector<char> &TCPReceive::Receive() { m_recBuffer.clear(); return m_recBuffer; }
bool TCPReceive::Connected() { return false; }
void TCPReceive::ListenFunc() {}
