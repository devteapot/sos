#!/usr/bin/env python3

from __future__ import annotations

import importlib.util
import io
import socket
import subprocess
import sys
import threading
import time
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location(
    "core_dev_connect_bridge", ROOT / "tools" / "core-dev-connect-bridge.py"
)
assert SPEC is not None and SPEC.loader is not None
bridge_module = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(bridge_module)


class BridgeHarness:
    def __init__(self) -> None:
        self.upstream_peer: socket.socket | None = None
        self.events_output = io.StringIO()

        def connector() -> socket.socket:
            bridge_end, peer_end = socket.socketpair()
            self.upstream_peer = peer_end
            return bridge_end

        self.server = bridge_module.ConnectBridge(
            connector,
            bridge_module.EventLog(self.events_output),
        )
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()

    def close(self) -> None:
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=2)
        if self.upstream_peer is not None:
            self.upstream_peer.close()

    def request(self, payload: bytes) -> tuple[socket.socket, bytes]:
        client = socket.create_connection(self.server.server_address, timeout=2)
        client.sendall(payload)
        return client, client.recv(4096)

    def events(self) -> list[str]:
        return self.events_output.getvalue().splitlines()

    def wait_for_event(self, prefix: str, timeout_seconds: float = 2.0) -> list[str]:
        deadline = time.monotonic() + timeout_seconds
        while time.monotonic() < deadline:
            events = self.events()
            if any(event.startswith(prefix) for event in events):
                return events
            time.sleep(0.01)
        return self.events()


class ConnectBridgeTest(unittest.TestCase):
    def setUp(self) -> None:
        self.harness = BridgeHarness()

    def tearDown(self) -> None:
        self.harness.close()

    def test_binds_only_ipv4_loopback(self) -> None:
        self.assertEqual(self.harness.server.server_address[0], "127.0.0.1")

    def test_rejects_non_allowlisted_and_malformed_requests(self) -> None:
        cases = [
            (b"GET / HTTP/1.1\r\nHost: openrouter.ai:443\r\n\r\n", b" 405 "),
            (
                b"CONNECT api.openai.com:443 HTTP/1.1\r\nHost: api.openai.com:443\r\n\r\n",
                b" 403 ",
            ),
            (
                b"CONNECT openrouter.ai:80 HTTP/1.1\r\nHost: openrouter.ai:80\r\n\r\n",
                b" 403 ",
            ),
            (
                b"CONNECT openrouter.ai:443 HTTP/1.0\r\nHost: openrouter.ai:443\r\n\r\n",
                b" 400 ",
            ),
            (b"CONNECT openrouter.ai:443 HTTP/1.1\r\n\r\n", b" 400 "),
            (
                b"CONNECT openrouter.ai:443 HTTP/1.1\r\nHost: openrouter.ai:443\r\n"
                b"Content-Length: 6\r\n\r\nsecret",
                b" 400 ",
            ),
        ]
        for request, expected in cases:
            with self.subTest(request=request):
                client, response = self.harness.request(request)
                self.assertIn(expected, response)
                client.close()
        events = self.harness.events()
        self.assertEqual(events.count("bridge_event=connection_accepted"), len(cases))
        self.assertEqual(
            sum(event.startswith("bridge_event=request_rejected status=") for event in events),
            len(cases),
        )

    def test_relays_opaque_bytes_in_both_directions_without_output(self) -> None:
        stdout = io.StringIO()
        stderr = io.StringIO()
        with redirect_stdout(stdout), redirect_stderr(stderr):
            client, response = self.harness.request(
                b"CONNECT openrouter.ai:443 HTTP/1.1\r\n"
                b"Host: openrouter.ai:443\r\n\r\n"
            )
            self.assertEqual(response, b"HTTP/1.1 200 Connection Established\r\n\r\n")
            assert self.harness.upstream_peer is not None
            secret = b"synthetic-secret-body-never-log"
            client.sendall(secret)
            self.assertEqual(self.harness.upstream_peer.recv(len(secret)), secret)
            ciphertext = b"\x16\x03\x03opaque-tls-record"
            self.harness.upstream_peer.sendall(ciphertext)
            self.assertEqual(client.recv(len(ciphertext)), ciphertext)
            client.close()
        self.assertEqual(stdout.getvalue(), "")
        self.assertEqual(stderr.getvalue(), "")
        events = self.harness.wait_for_event("bridge_event=relay_terminal ")
        self.assertIn("bridge_event=connection_accepted", events)
        self.assertIn(
            "bridge_event=connect_accepted authority=openrouter.ai:443",
            events,
        )
        self.assertIn("bridge_event=upstream_connected", events)
        self.assertIn(
            "bridge_event=relay_started direction=device_to_upstream",
            events,
        )
        self.assertIn(
            "bridge_event=relay_started direction=upstream_to_device",
            events,
        )
        terminal = next(event for event in events if event.startswith("bridge_event=relay_terminal "))
        self.assertIn(f"device_to_upstream_bytes={len(secret)}", terminal)
        self.assertIn(f"upstream_to_device_bytes={len(ciphertext)}", terminal)
        self.assertNotIn("synthetic-secret", "\n".join(events))
        self.assertNotIn("opaque-tls-record", "\n".join(events))

    def test_real_jitless_native_http_proxy_sends_connect_before_tls(self) -> None:
        proxy = "http://{}:{}".format(*self.harness.server.server_address)
        modules = ROOT / "services" / "sos-agent" / "node_modules"
        node_fetch = modules / "node-fetch" / "src" / "index.js"
        https_proxy_agent = modules / "https-proxy-agent"
        script = """
const { pathToFileURL } = require("node:url");
const { HttpsProxyAgent } = require(process.argv[2]);
(async () => {
  const { default: fetch } = await import(pathToFileURL(process.argv[1]).href);
  const agent = new HttpsProxyAgent(process.argv[3]);
  await fetch("https://openrouter.ai/api/v1/models", { agent });
})().then(() => process.exit(2), () => process.exit(0));
"""
        process = subprocess.Popen(
            [
                "node",
                "--jitless",
                "-e",
                script,
                str(node_fetch),
                str(https_proxy_agent),
                proxy,
            ],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        try:
            events = self.harness.wait_for_event("bridge_event=upstream_connected")
            self.assertIn(
                "bridge_event=connect_accepted authority=openrouter.ai:443",
                events,
            )
            assert self.harness.upstream_peer is not None
            self.harness.upstream_peer.settimeout(2)
            tls_record = self.harness.upstream_peer.recv(4096)
            self.assertTrue(tls_record.startswith(b"\x16\x03"))
            self.harness.upstream_peer.close()
            self.harness.upstream_peer = None
            stdout, stderr = process.communicate(timeout=2)
            self.assertEqual(process.returncode, 0)
            self.assertEqual(stdout, b"")
            self.assertTrue(
                stderr == b"" or b"disabling flag --expose_wasm" in stderr
            )
        finally:
            if process.poll() is None:
                process.kill()
                process.wait(timeout=2)
            if process.stdout is not None:
                process.stdout.close()
            if process.stderr is not None:
                process.stderr.close()


if __name__ == "__main__":
    unittest.main()
