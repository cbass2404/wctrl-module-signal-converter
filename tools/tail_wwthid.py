"""Follow WWTHID.log and keep every non-InputData line, surviving the log's wrap.

  python tools/tail_wwthid.py <out> [--seconds N]

SimAppPro truncates WWTHID.log in place once it reaches ~43 MB, which with the
joystick polls from every panel is under a minute. Polling fast and restarting
from 0 when the file shrinks keeps the sends and accepts we care about.
"""
import os
import sys
import time

LOG = os.path.expandvars(r"%APPDATA%\WWTHID\SimAppPro\WWTHID.log")


def main() -> None:
    out_path = sys.argv[1]
    limit = float(sys.argv[sys.argv.index("--seconds") + 1]) if "--seconds" in sys.argv else 3600
    pos = 0
    partial = b""
    wraps = 0
    kept = 0
    deadline = time.time() + limit
    with open(out_path, "ab") as out:
        while time.time() < deadline:
            try:
                size = os.path.getsize(LOG)
            except OSError:
                time.sleep(0.05)
                continue
            if size < pos:
                wraps += 1
                pos = 0
                partial = b""
                out.write(f"# --- log wrapped ({wraps}) at {time.strftime('%H:%M:%S')} ---\n".encode())
            if size > pos:
                with open(LOG, "rb") as f:
                    f.seek(pos)
                    chunk = f.read(size - pos)
                pos += len(chunk)
                lines = (partial + chunk).split(b"\n")
                partial = lines.pop()
                for line in lines:
                    if b"InputData:" not in line:
                        out.write(line + b"\n")
                        kept += 1
                out.flush()
            time.sleep(0.02)
    print(f"kept {kept} lines, {wraps} wraps")


if __name__ == "__main__":
    main()
