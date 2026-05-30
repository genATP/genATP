"""genATP Python SDK — the unified web-access layer for AI agents.

The 5-line PRD quickstart::

    from genatp import Agent

    agent = Agent(model="claude-3-5-sonnet-latest")
    result = agent.run("Find the cheapest direct flight from SFO to JFK next Friday.")
    print(result.summary)

Public surface (everything in :data:`__all__`) is type-checked and stable
under semver from v0.2.0 onwards. The native module ``genatp._native`` is
deliberately *not* part of the public API — its symbols may change between
patch releases.
"""

from __future__ import annotations

from genatp._native import build_info as _build_info
from genatp._native import version as _native_version
from genatp._types import (
    BudgetSpend,
    ProviderInfo,
    RunResult,
    ToolSpec,
)
from genatp.agent import Agent
from genatp.browser import BrowserSession
from genatp.errors import GenAtpError, ProviderError, RunError
from genatp.providers import resolve_provider_from_env


def _package_version() -> str:
    """Prefer the wheel metadata version (pyproject) over the Rust crate."""
    try:
        from importlib.metadata import version as _meta_version

        return _meta_version("genatp")
    except Exception:
        return _native_version()


__version__ = _package_version()


def build_info() -> dict[str, object]:
    """Return a dict describing the build — useful for ``genatp doctor``.

    The keys mirror :func:`genatp._native.build_info`:

    * ``genatp_python`` — Python-SDK crate version.
    * ``genatp_core``, ``genatp_llm``, ``genatp_browser``,
      ``genatp_perception``, ``genatp_mcp`` — workspace crate versions.
    * ``mcp_tools`` — list of MCP tool names exposed by this build.
    * ``providers`` — list of supported provider kinds (anthropic,
      openai, nim, ollama, mock).
    """
    return dict(_build_info())


__all__ = [
    "Agent",
    "BrowserSession",
    "BudgetSpend",
    "GenAtpError",
    "ProviderError",
    "ProviderInfo",
    "RunError",
    "RunResult",
    "ToolSpec",
    "__version__",
    "build_info",
    "resolve_provider_from_env",
]
