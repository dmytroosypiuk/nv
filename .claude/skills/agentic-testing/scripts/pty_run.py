#!/usr/bin/env python3
"""Drives a real interactive Claude Code session through a pty, one message at a time, and
reports from the session log what the model did. Standard library only. This is a
developer tool: it is not installed and nv does not need it.

usage: pty_run.py <name> <message>...
env:   MODEL (default haiku), TURN_TIMEOUT (seconds to wait for one answer, default 300)

Store:  /tmp/nv-agentic/store/<name>  (new and empty; the model is copied in)
Folder: /tmp/nv-agentic/work/<name>   (the session starts here)

The trust dialog is answered "yes" for this folder, which the script makes. Every
permission prompt is DENIED and reported: this script never approves anything for the user.
"""
import fcntl
import glob
import json
import os
import pty
import re
import select
import shutil
import signal
import struct
import subprocess
import sys
import termios
import time
import uuid

BASE = "/tmp/nv-agentic"
if len(sys.argv) < 3:
    sys.exit(__doc__)
name, *messages = sys.argv[1:]
store = f"{BASE}/store/{name}"
cwd = f"{BASE}/work/{name}"
model = os.environ.get("MODEL", "haiku")
shutil.rmtree(store, ignore_errors=True)
os.makedirs(store)
shutil.copytree(os.path.expanduser("~/.nv/models"), f"{store}/models")
os.makedirs(cwd, exist_ok=True)
session = str(uuid.uuid4())
env = {
    **os.environ,
    "NV_HOME": store,
    "TERM": "xterm-256color",
    # A session started from inside Claude Code keeps no transcript without this.
    "CLAUDE_CODE_FORCE_SESSION_PERSISTENCE": "1",
}
env.pop("CLAUDECODE", None)

pid, fd = pty.fork()
if pid == 0:
    os.chdir(cwd)
    os.execvpe("claude", ["claude", "--model", model, "--session-id", session], env)
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 50, 200, 0, 0))

ANSI = rb"\x1b\[[0-9;?<>]*[a-zA-Z]|\x1b\][^\x07]*\x07|\x1b[()][A-B0-9]"
screen = b""
events = []
last_dialog = 0.0


def drain(seconds):
    """Reads the terminal. Answers the trust dialog; denies a permission prompt."""
    global screen, last_dialog
    end = time.time() + seconds
    while time.time() < end:
        ready, _, _ = select.select([fd], [], [], 0.2)
        if ready:
            try:
                data = os.read(fd, 65536)
            except OSError:
                return
            screen = (screen + data)[-20000:]
    # The terminal draws spaces as cursor moves: compare the text without any.
    tail = re.sub(ANSI + rb"|\s", b"", screen[-4000:]).decode("utf8", "ignore")
    if "Itrustthisfolder" in tail and time.time() - last_dialog > 20:
        last_dialog = time.time()
        os.write(fd, b"\x1bOB")  # the default is "No, exit": move to "Yes, I trust"
        time.sleep(0.5)
        os.write(fd, b"\r")
        events.append("trust dialog answered yes for the folder this script made")
        screen = b""
    elif "Doyouwantto" in tail:
        os.write(fd, b"\x1b")
        events.append("PERMISSION PROMPT DENIED: " + tail[tail.rfind("Doyouwantto"):][:300])
        screen = b""


def entries():
    found = glob.glob(os.path.expanduser(f"~/.claude/projects/*work-{name}/{session}.jsonl"))
    if not found:
        return [], None
    out = []
    for line in open(found[0]):
        try:
            out.append(json.loads(line))
        except ValueError:
            pass
    return out, found[0]


def is_prompt(entry):
    content = entry.get("message", {}).get("content")
    return entry.get("type") == "user" and isinstance(content, str) and not entry.get("isMeta")


def turn_finished(expected):
    """The prompt is in the log, the last entry ended the turn, and the log is quiet."""
    log, path = entries()
    if not path or sum(map(is_prompt, log)) < expected:
        return False
    last = [e for e in log if e.get("type") in ("user", "assistant")][-1]
    done = last.get("type") == "assistant" and last["message"].get("stop_reason") == "end_turn"
    return done and time.time() - os.path.getmtime(path) > 6


drain(12)  # start-up and the trust dialog
drain(8)  # the dialog closes and the prompt appears
for number, message in enumerate(messages, 1):
    os.write(fd, message.encode())
    drain(1)
    os.write(fd, b"\r")
    deadline = time.time() + int(os.environ.get("TURN_TIMEOUT", "300"))
    while time.time() < deadline:
        drain(3)
        if turn_finished(number):
            break
    else:
        text = re.sub(ANSI, b" ", screen[-3000:]).decode("utf8", "ignore")
        events.append(f"TIMEOUT waiting for message {number}; screen: " + re.sub(r"\s+", " ", text)[-500:])
    drain(2)

os.write(fd, b"/exit\r")
drain(3)
try:
    os.kill(pid, signal.SIGTERM)
    os.waitpid(pid, 0)
except OSError:
    pass

# ---- the report, from the session log
log, _ = entries()
print(f"##### {name} ({model}): store {store}")
for entry in log:
    attachment = entry.get("attachment", {})
    if attachment.get("type") == "skill_listing":
        listing = attachment.get("content", "")
        for skill in ("nv-capture", "nv-recall"):
            line = next((l for l in listing.splitlines() if l.startswith(f"- {skill}")), "")
            described = line.startswith(f"- {skill}:")
            print(f"  listing: {skill} {'has a description' if described else 'NAMES ONLY'}")
number = 0
for entry in log:
    content = entry.get("message", {}).get("content")
    if is_prompt(entry):
        number += 1
        print(f"\n== message {number}: {content[:80]}")
    elif entry.get("type") == "user" and entry.get("isMeta") and isinstance(content, list):
        if "nv-capture" in json.dumps(content) or "nv-recall" in json.dumps(content):
            print("  (skill text delivered)")
    elif entry.get("type") == "assistant" and isinstance(content, list):
        for block in content:
            if block["type"] == "tool_use":
                tool, args = block["name"], block["input"]
                if tool == "Skill":
                    print(f"  SKILL  {args.get('skill')}")
                elif tool == "Bash":
                    print("  BASH   " + args["command"].replace("\n", "\n         "))
                elif tool == "Read":
                    print(f"  READ   {args.get('file_path', '')}")
                else:
                    print(f"  {tool.upper()} {json.dumps(args)[:110]}")
            elif block["type"] == "text" and block["text"].strip():
                print("  SAYS   " + block["text"].strip().replace("\n", "\n         ")[:700])
for event in events:
    print("  EVENT", event)
print("\n----- the store afterwards")
store_env = {**os.environ, "NV_HOME": store}
for command in (["search", "--since", "2000-01-01", "--limit", "50"], ["people", "list"], ["today"]):
    out = subprocess.run(["nv", *command], env=store_env, capture_output=True, text=True)
    print(f"$ nv {' '.join(command)}\n{out.stdout}{out.stderr}")
