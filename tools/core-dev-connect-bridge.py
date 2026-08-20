#!/usr/bin/env python3
"""Loopback-only HTTP CONNECT bridge for the bounded Core-dev smoke request."""

from __future__ import annotations

import socket
import socketserver
import sys
import threading
from collections.abc import Callable
from pathlib import Path
from typing import TextIO


LISTEN_HOST = "127.0.0.1"
ALLOWED_AUTHORITY = "openrouter.ai:443"
UPSTREAM_HOST = "openrouter.ai"
UPSTREAM_PORT = 443
ALLOWED_HOST_HEADERS = frozenset((UPSTREAM_HOST, ALLOWED_AUTHORITY))
MAX_REQUEST_BYTES = 8192
IO_TIMEOUT_SECONDS = 15
RELAY_TIMEOUT_SECONDS = 300


class EventLog:
    """Writes only fixed bridge phases, categories, statuses, and byte counts."""

    def __init__(self, output: TextIO | None = None) -> None:
        self.output = output
        self.lock = threading.Lock()

    def record(self, event: str, **fields: str | int) -> None:
        if self.output is None:
            return
        values = " ".join(f"{name}={value}" for name, value in fields.items())
        line = f"bridge_event={event}{' ' if values else ''}{values}\n"
        with self.lock:
            self.output.write(line)
            self.output.flush()


def connect_upstream() -> socket.socket:
    return socket.create_connection((UPSTREAM_HOST, UPSTREAM_PORT), IO_TIMEOUT_SECONDS)


class ConnectBridge(socketserver.ThreadingTCPServer):
    allow_reuse_address = False
    daemon_threads = True

    def __init__(
        self,
        connector: Callable[[], socket.socket] = connect_upstream,
        events: EventLog | None = None,
    ) -> None:
        self.connector = connector
        self.events = events or EventLog()
        super().__init__((LISTEN_HOST, 0), ConnectHandler)

    def handle_error(self, request: socket.socket, client_address: object) -> None:
        self.events.record("handler_failure", category="internal")
        del request, client_address


class ConnectHandler(socketserver.BaseRequestHandler):
    request: socket.socket
    server: ConnectBridge

    def handle(self) -> None:
        phase = "read_connect"
        self.server.events.record("connection_accepted")
        self.request.settimeout(IO_TIMEOUT_SECONDS)
        try:
            request = self._read_request()
            status = self._validate_request(request)
            if status is not None:
                self.server.events.record("request_rejected", status=status)
                self._reject(status)
                return
            self.server.events.record("connect_accepted", authority=ALLOWED_AUTHORITY)
            phase = "connect_upstream"
            upstream = self.server.connector()
            try:
                self.server.events.record("upstream_connected")
                phase = "confirm_connect"
                self.request.sendall(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                upstream.settimeout(RELAY_TIMEOUT_SECONDS)
                self.request.settimeout(RELAY_TIMEOUT_SECONDS)
                phase = "relay"
                device_bytes, upstream_bytes = self._relay(upstream)
                self.server.events.record(
                    "relay_terminal",
                    device_to_upstream_bytes=device_bytes,
                    upstream_to_device_bytes=upstream_bytes,
                )
            finally:
                upstream.close()
        except TimeoutError:
            self.server.events.record("bridge_failure", phase=phase, category="timeout")
            return
        except ConnectionError:
            self.server.events.record("bridge_failure", phase=phase, category="connection")
            return
        except OSError:
            self.server.events.record("bridge_failure", phase=phase, category="os")
            return

    def _read_request(self) -> bytes:
        data = bytearray()
        while b"\r\n\r\n" not in data:
            chunk = self.request.recv(min(1024, MAX_REQUEST_BYTES + 1 - len(data)))
            if not chunk:
                raise ConnectionError("incomplete request")
            data.extend(chunk)
            if len(data) > MAX_REQUEST_BYTES:
                raise ConnectionError("request too large")
        return bytes(data)

    @staticmethod
    def _validate_request(request: bytes) -> int | None:
        header, separator, trailing = request.partition(b"\r\n\r\n")
        if not separator or trailing:
            return 400
        try:
            lines = header.decode("ascii").split("\r\n")
        except UnicodeDecodeError:
            return 400
        if not lines or len(lines) > 64:
            return 400
        parts = lines[0].split(" ")
        if len(parts) != 3 or parts[2] != "HTTP/1.1":
            return 400
        method, authority, _ = parts
        if method != "CONNECT":
            return 405
        if authority != ALLOWED_AUTHORITY:
            return 403
        hosts = []
        for line in lines[1:]:
            name, colon, value = line.partition(":")
            if not colon or not name or any(character.isspace() for character in name):
                return 400
            if any(ord(character) < 32 or ord(character) == 127 for character in value):
                return 400
            lowered = name.lower()
            stripped = value.strip()
            if lowered == "host":
                hosts.append(stripped)
            if lowered == "content-length" and stripped != "0":
                return 400
            if lowered == "transfer-encoding":
                return 400
        if len(hosts) != 1 or hosts[0] not in ALLOWED_HOST_HEADERS:
            return 400
        return None

    def _reject(self, status: int) -> None:
        reason = {400: b"Bad Request", 403: b"Forbidden", 405: b"Method Not Allowed"}[status]
        self.request.sendall(
            b"HTTP/1.1 "
            + str(status).encode("ascii")
            + b" "
            + reason
            + b"\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
        )

    def _relay(self, upstream: socket.socket) -> tuple[int, int]:
        def pump(
            source: socket.socket,
            destination: socket.socket,
            direction: str,
        ) -> int:
            transferred = 0
            try:
                while chunk := source.recv(65536):
                    destination.sendall(chunk)
                    if transferred == 0:
                        self.server.events.record("relay_started", direction=direction)
                    transferred += len(chunk)
            except (ConnectionError, OSError, TimeoutError):
                pass
            try:
                destination.shutdown(socket.SHUT_WR)
            except OSError:
                pass
            return transferred

        upstream_to_device = [0]
        def reverse_pump() -> None:
            upstream_to_device[0] = pump(
                upstream,
                self.request,
                "upstream_to_device",
            )
            try:
                self.request.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass

        reverse = threading.Thread(target=reverse_pump, daemon=True)
        reverse.start()
        device_to_upstream = pump(
            self.request,
            upstream,
            "device_to_upstream",
        )
        try:
            upstream.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        reverse.join()
        return device_to_upstream, upstream_to_device[0]


def main(argv: list[str]) -> int:
    if len(argv) != 2 or argv[0] != "--events" or not argv[1]:
        return 64
    events_path = Path(argv[1])
    with events_path.open("x", encoding="ascii", buffering=1) as output:
        with ConnectBridge(events=EventLog(output)) as bridge:
            host, port = bridge.server_address
            print(
                f"CORE_DEV_CONNECT_BRIDGE_READY host={host} port={port} "
                f"authority={ALLOWED_AUTHORITY}",
                flush=True,
            )
            bridge.serve_forever(poll_interval=0.1)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
