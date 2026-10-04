// No sockets in this build (see SDLIncludes.h): the network board never connects.
#include "Network/TCPSendAsync.h"

TCPSendAsync::TCPSendAsync(std::string &ip, int port) : m_ip(ip), m_port(port), m_socket(nullptr), m_hasData(false) {}
TCPSendAsync::~TCPSendAsync() {}
bool TCPSendAsync::Send(const void *data, int length) { (void)data; (void)length; return false; }
bool TCPSendAsync::Connect() { return false; }
bool TCPSendAsync::Connected() { return false; }
void TCPSendAsync::SendThread() {}
