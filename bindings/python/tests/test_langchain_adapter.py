"""Tests for the LangChain adapter.

Requires both ``langchain-core`` and a built ``genatp._native``. The
adapter is exercised at the wiring level only — actual end-to-end
agent-loop tests live in :file:`test_agent_mock.py`.
"""

from __future__ import annotations

import importlib
from typing import Any

import pytest

pytestmark = pytest.mark.integration

if importlib.util.find_spec("langchain_core") is None:  # pragma: no cover
    pytest.skip("langchain-core not installed", allow_module_level=True)


def test_as_langchain_tools_exposes_nine_tools(monkeypatch: pytest.MonkeyPatch) -> None:
    """A monkey-patched BrowserSession proves we don't launch Chrome."""
    from genatp import langchain as adapter

    calls: list[tuple[str, dict[str, Any]]] = []

    class FakeSession:
        def navigate(self, url: str) -> str:
            calls.append(("navigate", {"url": url}))
            return url

        def snapshot(self) -> dict[str, object]:
            calls.append(("snapshot", {}))
            return {"nodes": []}

        def screenshot(self) -> bytes:
            calls.append(("screenshot", {}))
            return b"\x89PNG"

        def call_tool(self, name: str, args: dict[str, object]) -> str:
            calls.append((name, dict(args)))
            return f"ok:{name}"

        def close(self) -> None:
            calls.append(("close", {}))

    fake = FakeSession()
    tools = adapter.as_langchain_tools(session=fake)  # type: ignore[arg-type]

    assert len(tools) == 9
    names = sorted(t.name for t in tools)
    assert names == sorted(
        [
            "navigate",
            "snapshot",
            "click",
            "fill",
            "extract",
            "scroll",
            "screenshot",
            "get_url",
            "go_back",
        ]
    )

    # Each tool's `func` should round-trip through the fake session.
    navigate_tool = next(t for t in tools if t.name == "navigate")
    result = navigate_tool.invoke({"url": "https://example.com"})
    assert result == "ok:navigate"
    assert ("navigate", {"url": "https://example.com"}) in calls


def test_session_attached_to_tool_metadata() -> None:
    from genatp import langchain as adapter

    class FakeSession:
        def call_tool(self, name: str, args: dict[str, object]) -> str:
            return ""

        def snapshot(self) -> dict[str, object]:
            return {}

        def screenshot(self) -> bytes:
            return b""

        def close(self) -> None:
            pass

    fake = FakeSession()
    tools = adapter.as_langchain_tools(session=fake)  # type: ignore[arg-type]
    for t in tools:
        assert t.metadata is not None
        assert t.metadata.get("session") is fake
