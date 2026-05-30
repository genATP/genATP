"""LlamaIndex adapter — exposes the nine MCP tools as LlamaIndex tools.

Optional dependency — install via ``pip install genatp[llamaindex]``.

Usage::

    from genatp.llamaindex import as_llamaindex_tools
    from llama_index.agent import ReActAgent

    tools = as_llamaindex_tools()
    agent = ReActAgent.from_tools(tools, llm=llm, verbose=True)

The adapter mirrors :mod:`genatp.langchain` — one shared
:class:`genatp.BrowserSession` is wrapped into nine LlamaIndex
``FunctionTool`` instances.
"""

from __future__ import annotations

import json
from typing import Any

from genatp.browser import BrowserSession
from genatp.errors import GenAtpError
from genatp.tools import list_tools

try:
    from llama_index.core.tools import FunctionTool, ToolMetadata
except ImportError as exc:  # pragma: no cover — optional dep
    raise ImportError(
        "llama-index-core is required for genatp.llamaindex — "
        "install with `pip install genatp[llamaindex]` or "
        "`pip install llama-index-core`."
    ) from exc


def as_llamaindex_tools(session: BrowserSession | None = None) -> list[FunctionTool]:
    """Return the nine MCP tools as LlamaIndex :class:`FunctionTool` objects.

    Args:
        session: Optionally inject an existing session — useful for
            tests. ``None`` (the default) creates a fresh session.

    Returns:
        A list of nine LlamaIndex ``FunctionTool`` instances, in the
        same order as the MCP ``tools/list`` response.

    The shared session is attached as ``tool._session`` so callers can
    close it::

        tools = as_llamaindex_tools()
        try:
            ...
        finally:
            tools[0]._session.close()  # type: ignore[attr-defined]
    """
    bs = session or BrowserSession()
    tools: list[FunctionTool] = []
    for spec in list_tools():
        tools.append(_build_tool(bs, spec.name, spec.description, spec.input_schema))
    for t in tools:
        t._session = bs  # type: ignore[attr-defined]
    return tools


def _build_tool(
    bs: BrowserSession,
    name: str,
    description: str,
    input_schema: dict[str, Any],
) -> FunctionTool:
    metadata = ToolMetadata(
        name=name,
        description=description,
        fn_schema=None,  # rely on inferred signature; full schemas land in v0.6
    )

    def _call(**kwargs: Any) -> str:
        filtered = {k: v for k, v in kwargs.items() if v is not None}
        if name == "snapshot":
            return json.dumps(bs.snapshot())
        if name == "screenshot":
            data = bs.screenshot()
            return f"<binary image/png: {len(data)} bytes>"
        try:
            return bs.call_tool(name, filtered)
        except GenAtpError as exc:
            return f"error: {exc}"

    return FunctionTool.from_defaults(fn=_call, name=name, description=description)


__all__ = ["as_llamaindex_tools"]
