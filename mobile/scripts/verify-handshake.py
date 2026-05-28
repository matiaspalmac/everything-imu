"""
Send the exact handshake/rotation bytes that the Android app sends and check
if SlimeVR Server replies. Helps verify the wire layout without involving
the phone.
"""
import socket
import struct
import sys
import time

HOST = "192.168.1.253"
PORT = 6969

def build_handshake(seq=0, mac=b"\x02\x01\x02\x03\x04\x05", firmware=b"owoTrack8"):
    board_type = 13
    imu_type = 0
    mcu_type = 3
    firmware_build = 8

    payload = b""
    payload += struct.pack(">I", board_type)
    payload += struct.pack(">I", imu_type)
    payload += struct.pack(">I", mcu_type)
    payload += struct.pack(">III", 0, 0, 0)  # imu_info slots
    payload += struct.pack(">I", firmware_build)
    payload += struct.pack(">B", len(firmware))
    payload += firmware
    assert len(mac) == 6
    payload += mac
    payload += b"\xff"

    header = struct.pack(">I", 3) + struct.pack(">Q", seq)
    return header + payload


def build_rotation(seq, x, y, z, w):
    payload = struct.pack(">ffff", x, y, z, w)
    header = struct.pack(">I", 1) + struct.pack(">Q", seq)
    return header + payload


def build_heartbeat(seq):
    return struct.pack(">I", 0) + struct.pack(">Q", seq) + b"\x00"


def main():
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.settimeout(2.0)
    sock.bind(("0.0.0.0", 0))

    hs = build_handshake()
    print(f"sending handshake ({len(hs)} bytes): {hs.hex()}")
    sock.sendto(hs, (HOST, PORT))

    try:
        data, addr = sock.recvfrom(1024)
        print(f"\n[OK] reply {len(data)} bytes from {addr}: {data!r}")
        if data[:1] == b"\x03":
            print(f"  msg_type 3 (handshake) — server says: {data[1:].split(b'\\x00', 1)[0].decode('ascii', errors='replace')}")
    except socket.timeout:
        print("[FAIL] no reply within 2s")
        return 1

    seq = 1
    for i in range(20):
        x, y, z, w = 0.0, 0.0, 0.0, 1.0
        sock.sendto(build_rotation(seq, x, y, z, w), (HOST, PORT))
        seq += 1
        time.sleep(0.05)
        if i % 5 == 0:
            sock.sendto(build_heartbeat(seq), (HOST, PORT))
            seq += 1

    print(f"sent {seq} additional packets (rotation + heartbeat). check SlimeVR Server UI.")
    sock.settimeout(0.5)
    while True:
        try:
            data, addr = sock.recvfrom(1024)
            print(f"  inbound from {addr}: tag={struct.unpack('>I', data[:4])[0]} len={len(data)}")
        except socket.timeout:
            break

    return 0


if __name__ == "__main__":
    sys.exit(main())
