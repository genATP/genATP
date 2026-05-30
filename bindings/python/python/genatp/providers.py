"""Provider discovery helpers — wraps :mod:`genatp._native`.

Most users never call into this module directly; :class:`genatp.Agent`
auto-resolves a provider from the environment. The helpers are exposed
for diagnostic scripts and for the LangChain / LlamaIndex examples that
want to print which backend they're hitting.
"""

from __future__ import annotations

from typing import Final

from genatp._native import resolve_provider_from_env as _resolve_native

from genatp._types import ProviderInfo
from genatp.errors import ProviderError

SUPPORTED_PROVIDERS: Final[tuple[str, ...]] = (
    "anthropic",
    "openai",
    "nim",
    "ollama",
    "mock",
)


def resolve_provider_from_env() -> ProviderInfo:
    """Inspect environment variables and return the resolved provider.

    See :file:`docs/CONFIGURATION.md` for the selection rules. Briefly:

    1. ``GENATP_LLM_PROVIDER`` if set wins.
    2. Otherwise the first credential present wins, in this order:
       ``ANTHROPIC_API_KEY``, ``NVIDIA_API_KEY``, ``OPENAI_API_KEY``.
    3. Falls back to a local Ollama daemon at ``127.0.0.1:11434``.

    Raises:
        ProviderError: If ``GENATP_LLM_PROVIDER`` is set to an unknown
            value or the selected backend's required credentials are
            missing.
    """
    try:
        native = _resolve_native()
    except Exception as exc:  # noqa: BLE001 — convert PyO3 RuntimeError
        raise ProviderError(str(exc)) from exc
    return ProviderInfo(kind=native.kind, name=native.name, model=native.model)
