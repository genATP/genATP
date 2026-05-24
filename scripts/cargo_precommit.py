#!/usr/bin/env python3
"""Pre-commit wrapper: resolve `cargo` on Windows (WSL bash in PATH) and Unix."""

from __future__ import annotations

import os
import shutil
import subprocess
import sys


def find_cargo() -> str | None:
    cargo = shutil.which("cargo")
    if cargo:
        return cargo

    if sys.platform == "win32":
        profile = os.environ.get("USERPROFILE", "")
        if profile:
            candidate = os.path.join(profile, ".cargo", "bin", "cargo.exe")
            if os.path.isfile(candidate):
                return candidate

    home = os.environ.get("HOME", "")
    if home:
        for name in ("cargo", "cargo.exe"):
            candidate = os.path.join(home, ".cargo", "bin", name)
            if os.path.isfile(candidate):
                return candidate

    return None


def main() -> int:
    cargo = find_cargo()
    if cargo is None:
        print("error: cargo not found. Install Rust: https://rustup.rs", file=sys.stderr)
        if sys.platform == "win32":
            print(
                "  expected: %USERPROFILE%\\.cargo\\bin\\cargo.exe",
                file=sys.stderr,
            )
        else:
            print("  expected: $HOME/.cargo/bin/cargo", file=sys.stderr)
        return 1

    return subprocess.call([cargo, *sys.argv[1:]])


if __name__ == "__main__":
    raise SystemExit(main())
