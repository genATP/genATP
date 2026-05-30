"""Shared pytest fixtures for the genATP Python SDK tests.

Marker convention (see :file:`pyproject.toml`):

* ``live``: real Chrome + real LLM. Gated on ``GENATP_PY_LIVE=1``.
* ``integration``: needs the native module compiled. Skipped when the
  module fails to import (handy for pure-Python lint runs in CI's
  ``lint`` job, where maturin hasn't built the wheel yet).
"""

from __future__ import annotations

import os
from importlib import util

import pytest


def _native_importable() -> bool:
    return util.find_spec("genatp._native") is not None


def pytest_collection_modifyitems(config: pytest.Config, items: list[pytest.Item]) -> None:
    """Skip ``integration`` tests when the native module isn't built."""
    skip_integration = pytest.mark.skip(
        reason="genatp._native not importable — build the wheel via `maturin develop`",
    )
    skip_live = pytest.mark.skip(
        reason="GENATP_PY_LIVE != 1 — set the env var to enable live tests",
    )
    native_present = _native_importable()
    live_on = os.environ.get("GENATP_PY_LIVE", "") == "1"

    for item in items:
        if "integration" in item.keywords and not native_present:
            item.add_marker(skip_integration)
        if "live" in item.keywords and not live_on:
            item.add_marker(skip_live)
