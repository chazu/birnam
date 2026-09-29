#!/usr/bin/env python3
"""Wire peer for injecting a parent-side native failure after a valid reply."""
import socket
import struct
import sys

channel = socket.socket(fileno=int(sys.argv[-1]))


def exact(length):
    result = b''
    while len(result) < length:
        part = channel.recv(length - len(result))
        if not part:
            sys.exit(0)
        result += part
    return result


channel.sendall(struct.pack('!4sHBBQI', b'BISO', 1, 1, 0, 0, 0))
while True:
    magic, version, kind, flags, request, length = struct.unpack('!4sHBBQI', exact(20))
    exact(length)
    if kind == 6:
        channel.sendall(struct.pack('!4sHBBQI', b'BISO', 1, 7, 0, request, 0))
        break
    assert (magic, version, kind, flags) == (b'BISO', 1, 3, 0)
    channel.sendall(struct.pack('!4sHBBQI', b'BISO', 1, 4, 0, request, 1) + b'\0')
