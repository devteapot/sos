#!/usr/bin/env python3

import os
import signal
import socket
import sys
import time


def option(name: str) -> str:
    index = sys.argv.index(name)
    return sys.argv[index + 1]


def listener(path: str) -> socket.socket:
    os.makedirs(os.path.dirname(path), exist_ok=True)
    try:
        os.unlink(path)
    except FileNotFoundError:
        pass
    server = socket.socket(socket.AF_UNIX)
    server.bind(path)
    server.listen(1)
    return server


def wait_for_stop() -> None:
    stopping = False

    def stop(_signum: int, _frame: object) -> None:
        nonlocal stopping
        stopping = True

    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    while not stopping:
        time.sleep(0.01)


name = os.path.basename(sys.argv[0])
if name == "sos-revision-supervisor":
    if len(sys.argv) >= 2 and sys.argv[1] == "status":
        print("1" * 64)
        raise SystemExit(0)
    raise SystemExit("unsupported revision-supervisor mock command")

if name == "sos-linux-session":
    runtime = option("--runtime-dir")
    root = option("--root")
    provider = listener(os.path.join(runtime, "provider-state.sock"))
    supervisor = listener(os.path.join(root, "run", "supervisor.sock"))
    print("linux_system_session_ready revision_id=" + "1" * 64 + " evidence=drm_page_flip", flush=True)
    time.sleep(0.75)
    provider.close()
    supervisor.close()
    print("linux_login_session_stopped reason=user_logout", flush=True)
    raise SystemExit(0)

if name == "sos-agent-authoring":
    authoring = listener(option("--listen-socket"))
    wait_for_stop()
    authoring.close()
    raise SystemExit(0)

if name == "node":
    arguments_file = os.environ["SOS_TEST_AGENT_ARGS_FILE"]
    with open(arguments_file, "w", encoding="utf-8") as output:
        output.write("\n".join(sys.argv[1:]) + "\n")
    agent = listener(option("--socket"))
    print("sos_agent_listening socket=" + option("--socket") + " model=faux", flush=True)
    wait_for_stop()
    agent.close()
    raise SystemExit(0)

wait_for_stop()
