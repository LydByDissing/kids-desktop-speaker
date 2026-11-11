#!/usr/bin/env python3
"""Send commands to CamillaDSP's websocket API and print the replies as JSON.

Usage: cdsp-ws.py HOST:PORT COMMAND [COMMAND...]
A command is a name ("GetState") or JSON ('{"SetVolume": -10}'). Standard
library only, so it runs anywhere the hardware tests run (DEC-036).
"""
import base64
import json
import os
import socket
import struct
import sys


def connect(addr):
    host, port = addr.rsplit(":", 1)
    sock = socket.create_connection((host, int(port)), timeout=5)
    key = base64.b64encode(os.urandom(16)).decode()
    sock.sendall(
        f"GET / HTTP/1.1\r\nHost: {addr}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
        f"Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n".encode()
    )
    head = b""
    while b"\r\n\r\n" not in head:
        chunk = sock.recv(1024)
        if not chunk:
            raise ConnectionError("closed during handshake")
        head += chunk
    if b" 101 " not in head.split(b"\r\n", 1)[0]:
        raise ConnectionError(head.split(b"\r\n", 1)[0].decode())
    return sock


def send(sock, text):
    data = text.encode()
    mask = os.urandom(4)
    header = bytes([0x81])
    if len(data) < 126:
        header += bytes([0x80 | len(data)])
    else:
        header += bytes([0x80 | 126]) + struct.pack(">H", len(data))
    sock.sendall(header + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(data)))


def recv_exact(sock, n):
    buf = b""
    while len(buf) < n:
        chunk = sock.recv(n - len(buf))
        if not chunk:
            raise ConnectionError("closed")
        buf += chunk
    return buf


def receive(sock):
    b0, b1 = recv_exact(sock, 2)
    length = b1 & 0x7F
    if length == 126:
        length = struct.unpack(">H", recv_exact(sock, 2))[0]
    elif length == 127:
        length = struct.unpack(">Q", recv_exact(sock, 8))[0]
    return recv_exact(sock, length).decode()


def main():
    addr, commands = sys.argv[1], sys.argv[2:]
    sock = connect(addr)
    for command in commands:
        send(sock, command if command.startswith("{") else json.dumps(command))
        print(receive(sock))


if __name__ == "__main__":
    main()
