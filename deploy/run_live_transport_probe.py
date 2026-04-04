#!/usr/bin/env python3
"""Thin wrapper for the hosted live transport probe."""

import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from rusaren_ops.live_transport_probe import main  # noqa: E402


if __name__ == "__main__":
    raise SystemExit(main(repo_root=REPO_ROOT))
