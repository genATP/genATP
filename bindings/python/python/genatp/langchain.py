"""LangChain adapter — exposes the nine MCP tools as LangChain BaseTools.

Optional dependency — install via ``pip install genatp[langchain]``.

Usage::

    from genatp.langchain import as_langchain_tools
    from langgraph.prebuilt import create_react_agent

    tools = as_langchain_tools()           # 9 production-ready tools
    agent = create_react_agent(model, tools=tools)

All adapters share **one** :class:`genatp.BrowserSession` so the
``@eN`` refs returned by ``snapshot`` resolve consistently across
subsequent ``click`` / ``fill`` calls.

The adapter is synchronous; LangChain's async interface delegates to
:func:`asyncio.to_thread`.
"""

from __future__ import annotations

import json
from typing import Any

from genatp.browser import BrowserSession
from genatp.errors import GenAtpError
from genatp.tools import list_tools

try:
    from langchain_core.tools import StructuredTool
except ImportError as exc:  # pragma: no cover — optional dep
    raise ImportError(
        "langchain-core is required for genatp.langchain — "
        "install with `pip install genatp[langchain]` or "
        "`pip install langchain-core`."
    ) from exc


def _json_schema_to_kwargs(schema: dict[str, Any]) -> dict[str, Any]:
    """Render a JSON-Schema object into LangChain's ``args_schema`` kwargs.

    LangChain accepts a JSON-Schema dict directly via the
    ``args_schema`` keyword (since ``langchain-core>=0.3``). We pass it
    through verbatim — the MCP descriptors are already strict object
    schemas with ``additionalProperties: false``.
    """
    return {
        "type": "object",
        "properties": schema.get("properties", {}),
        "required": schema.get("required", []),
        "additionalProperties": schema.get("additionalProperties", False),
    }


def as_langchain_tools(session: BrowserSession | None = None) -> list[StructuredTool]:
    """Return the nine MCP tools as LangChain ``StructuredTool`` instances.

    Args:
        session: Optionally inject an existing session — useful for
            tests, or when you want to pre-warm Chrome. ``None`` (the
            default) creates a fresh session that callers must close
            via :meth:`BrowserSession.close`.

    Returns:
        A list of nine :class:`langchain_core.tools.StructuredTool`
        objects, in the same order as the MCP ``tools/list`` response.

    The owning session is attached as ``tool.metadata["session"]`` so
    callers can close it cleanly::

        tools = as_langchain_tools()
        try:
            ...
        finally:
            tools[0].metadata["session"].close()
    """
    bs = session or BrowserSession()
    tools: list[StructuredTool] = []
    for spec in list_tools():
        tools.append(_build_tool(bs, spec.name, spec.description, spec.input_schema))
    # Attach the session to every tool for easy access.
    for t in tools:
        t.metadata = {**(t.metadata or {}), "session": bs}
    return tools


def _build_tool(
    bs: BrowserSession,
    name: str,
    description: str,
    input_schema: dict[str, Any],
) -> StructuredTool:
    args_schema = _json_schema_to_kwargs(input_schema)

    def _call(**kwargs: Any) -> str:
        # Filter out None args — LangChain sometimes passes them for
        # optional fields and the MCP layer is strict.
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

    return StructuredTool.from_function(
        func=_call,
        name=name,
        description=description,
        args_schema=args_schema,
    )


__all__ = ["as_langchain_tools"]
