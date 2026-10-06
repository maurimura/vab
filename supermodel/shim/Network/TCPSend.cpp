// No sockets in this build (see SDLIncludes.h): the network board never connects.
#include "Network/TCPSend.h"

TCPSend::TCPSend(std::string &ip, int port) : m_ip(ip), m_port(port), m_socket(nullptr) {}
TCPSend::~TCPSend() {}
bool TCPSend::Send(const void *data, int length) { (void)data; (void)length; return false; }
bool TCPSend::Connect() { return false; }
bool TCPSend::Connected() { return false; }
