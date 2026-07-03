import os
import signal
import subprocess
import time
import urllib.request
from pathlib import Path

import pytest

PORT = 8311
# The bundle is served under /angulito/, mirroring the GitHub Pages layout.
READY_URL = f"http://localhost:{PORT}/angulito/"
SERVE = Path(__file__).parent / "serve.sh"


def _is_up() -> bool:
    try:
        with urllib.request.urlopen(READY_URL, timeout=1):
            return True
    except OSError:
        return False


@pytest.fixture(scope="session", autouse=True)
def web_server():
    """Builds and serves the release bundle for the whole session.

    Outside CI an already running server (e.g. a manual serve.sh) is
    reused instead of rebuilding the bundle.
    """
    if not os.environ.get("CI") and _is_up():
        yield
        return

    # serve.sh runs dx and then the server as children of itself. A fresh
    # process group lets teardown catch the whole tree.
    process = subprocess.Popen(["bash", str(SERVE), str(PORT)], start_new_session=True)
    try:
        deadline = time.monotonic() + 300
        while not _is_up():
            if process.poll() is not None:
                raise RuntimeError(f"web server exited with code {process.returncode}")
            if time.monotonic() > deadline:
                raise TimeoutError(f"web server not reachable at {READY_URL}")
            time.sleep(0.5)
        yield
    finally:
        if process.poll() is None:
            os.killpg(os.getpgid(process.pid), signal.SIGTERM)
            process.wait(timeout=10)
