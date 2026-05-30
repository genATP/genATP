"""Exception hierarchy for the Python SDK.

Every error the native module raises is normalised into one of these
classes so callers can ``except GenAtpError`` without worrying about
PyO3's :class:`RuntimeError`.
"""

from __future__ import annotations


class GenAtpError(Exception):
    """Base class for every genATP-raised exception.

    Catching this gives consumers a single point of recovery for both
    provider failures (no API key, network error) and run-level errors
    (budget exceeded, browser crash).
    """


class ProviderError(GenAtpError):
    """Raised when an LLM provider cannot be configured or contacted.

    Common causes:

    * No `*_API_KEY` is set and no local Ollama daemon is reachable.
    * The configured provider returns a non-retryable error.
    * The provider id passed to :class:`genatp.Agent` is unknown.
    """


class RunError(GenAtpError):
    """Raised when an agent run itself fails for non-recoverable reasons.

    The orchestrator already surfaces ``aborted`` outcomes through
    :class:`genatp.RunResult`; only errors that escape the plan loop
    propagate as this exception.
    """
