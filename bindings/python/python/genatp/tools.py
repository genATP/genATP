"""Programmatic access to the MCP tool catalogue.

The native module exposes nine tools (``navigate``, ``snapshot``,
``click``, ``fill``, ``extract``, ``scroll``, ``screenshot``,
``get_url``, ``go_back``) — same names, schemas, and semantics as the
JSON-RPC ``tools/list`` response.

This module turns those descriptors into Python data classes that
LangChain and LlamaIndex can consume. End users typically import the
LangChain / LlamaIndex helpers directly; :func:`list_tools` is here for
introspection.
"""

from __future__ import annotations

import json
from typing import Final

from genatp._native import list_mcp_tools as _list_native
from genatp._types import ToolSpec


def list_tools() -> list[ToolSpec]:
    """Return every MCP tool the current native build exposes.

    The order is stable across calls and matches the ``tools/list``
    JSON-RPC response from :mod:`genatp_mcp`.
    """
    specs: list[ToolSpec] = []
    for t in _list_native():
        schema = json.loads(t.input_schema_json)
        specs.append(
            ToolSpec(
                name=t.name,
                description=t.description,
                input_schema=schema,
            )
        )
    return specs


SUPPORTED_TOOL_NAMES: Final[frozenset[str]] = frozenset(
    {
        "navigate",
        "snapshot",
        "click",
        "fill",
        "extract",
        "scroll",
        "screenshot",
        "get_url",
        "go_back",
    }
)
