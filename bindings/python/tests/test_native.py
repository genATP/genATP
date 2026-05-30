"""Smoke tests for the PyO3 native module.

These exercise everything that does NOT need a live Chrome instance or
a real LLM provider — version, tool listing, build info. Marked
``integration`` because they require ``genatp._native`` to be importable,
which means the maturin wheel has been built (or ``maturin develop``
has been run).
"""

from __future__ import annotations

import pytest

pytestmark = pytest.mark.integration


def test_import_native_module() -> None:
    """``genatp._native`` must be importable."""
    import genatp._native as _n  # noqa: F401


def test_top_level_import_exposes_public_surface() -> None:
    import genatp

    assert hasattr(genatp, "Agent")
    assert hasattr(genatp, "BrowserSession")
    assert hasattr(genatp, "RunResult")
    assert hasattr(genatp, "ProviderInfo")
    assert hasattr(genatp, "build_info")
    assert genatp.__version__.count(".") >= 2


def test_build_info_returns_expected_keys() -> None:
    import genatp

    info = genatp.build_info()
    for key in (
        "genatp_python",
        "genatp_core",
        "genatp_llm",
        "genatp_browser",
        "genatp_perception",
        "genatp_mcp",
        "mcp_tools",
        "providers",
    ):
        assert key in info, f"missing key {key!r} in build_info"
    assert "navigate" in info["mcp_tools"]
    assert "snapshot" in info["mcp_tools"]
    assert "mock" in info["providers"]


def test_list_tools_returns_nine_specs() -> None:
    from genatp.tools import SUPPORTED_TOOL_NAMES, list_tools

    specs = list_tools()
    names = {s.name for s in specs}
    assert names == set(SUPPORTED_TOOL_NAMES)
    # Every spec should have an object schema with a properties map.
    for s in specs:
        assert s.input_schema["type"] == "object"
        assert "properties" in s.input_schema


def test_resolve_provider_from_env_handles_missing_creds(monkeypatch: pytest.MonkeyPatch) -> None:
    """With no credentials set, the factory falls back to Ollama."""
    for var in (
        "GENATP_LLM_PROVIDER",
        "ANTHROPIC_API_KEY",
        "OPENAI_API_KEY",
        "NVIDIA_API_KEY",
    ):
        monkeypatch.delenv(var, raising=False)
    from genatp.providers import resolve_provider_from_env

    info = resolve_provider_from_env()
    assert info.kind == "ollama"
    assert info.model.startswith("llama")
