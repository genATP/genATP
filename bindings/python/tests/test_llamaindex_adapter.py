"""Tests for the LlamaIndex adapter."""

from __future__ import annotations

import importlib

import pytest

pytestmark = pytest.mark.integration

if importlib.util.find_spec("llama_index") is None:  # pragma: no cover
    pytest.skip("llama-index-core not installed", allow_module_level=True)


def test_as_llamaindex_tools_exposes_nine_tools() -> None:
    from genatp import llamaindex as adapter

    class FakeSession:
        def call_tool(self, name: str, args: dict[str, object]) -> str:  # noqa: ARG002
            return f"ok:{name}"

        def snapshot(self) -> dict[str, object]:
            return {"nodes": []}

        def screenshot(self) -> bytes:
            return b"\x89PNG"

        def close(self) -> None:
            pass

    fake = FakeSession()
    tools = adapter.as_llamaindex_tools(session=fake)  # type: ignore[arg-type]

    assert len(tools) == 9
    names = sorted(t.metadata.name for t in tools)
    assert "navigate" in names
    assert "snapshot" in names

    nav = next(t for t in tools if t.metadata.name == "navigate")
    out = nav(url="https://example.com")
    # FunctionTool wraps the return value in ToolOutput-like containers.
    text = getattr(out, "content", None) or str(out)
    assert "navigate" in text
