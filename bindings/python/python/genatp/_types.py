"""Pure-Python dataclasses surfaced from :mod:`genatp`.

These wrap the PyO3 result types so consumers never have to import from
:mod:`genatp._native` — handy because the native module's symbols are
not part of the public API.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Literal


@dataclass(frozen=True, slots=True)
class BudgetSpend:
    """Token + USD tally for a single :meth:`genatp.Agent.run`.

    Attributes:
        tokens: Total ``prompt + completion`` tokens charged by the LLM.
        usd: Cumulative spend in dollars. Zero for free providers
            (Ollama) and the mock provider.
        event_count: Number of entries recorded in the run's
            hash-chained event log.
    """

    tokens: int
    usd: float
    event_count: int


@dataclass(frozen=True, slots=True)
class RunResult:
    """Outcome of one :meth:`genatp.Agent.run` invocation.

    Attributes:
        outcome: ``"success"`` if the agent emitted a ``finish`` action,
            ``"aborted"`` if it hit a step/budget/wall limit,
            ``"error"`` if a non-recoverable error escaped the loop.
        summary: For ``success`` runs, the agent's natural-language
            summary. For ``aborted``/``error`` runs, the abort reason or
            error message.
        steps: Number of orchestrator iterations.
        spend: :class:`BudgetSpend` snapshot at termination.
    """

    outcome: Literal["success", "aborted", "error"]
    summary: str
    steps: int
    spend: BudgetSpend

    @property
    def ok(self) -> bool:
        """``True`` iff the agent finished successfully."""
        return self.outcome == "success"


@dataclass(frozen=True, slots=True)
class ProviderInfo:
    """Snapshot of the resolved LLM provider.

    Attributes:
        kind: One of ``"anthropic"``, ``"openai"``, ``"nim"``,
            ``"ollama"``, or ``"mock"``.
        name: The provider's stable name. Matches ``kind`` for every
            provider except NIM, which reports as ``"openai"`` since
            it shares OpenAI's wire format.
        model: Default model id the orchestrator will pass on
            ``CompletionRequest`` if none is overridden.
    """

    kind: str
    name: str
    model: str


@dataclass(frozen=True, slots=True)
class ToolSpec:
    """Metadata for one MCP browser tool.

    Mirrors the ``tools/list`` response of the MCP server. Used by
    :mod:`genatp.tools`, :mod:`genatp.langchain`, and
    :mod:`genatp.llamaindex` to build adapters.

    Attributes:
        name: Tool name (``navigate``, ``click``, …).
        description: Human-readable description sent to the LLM.
        input_schema: JSON-Schema (already parsed) for the tool's
            arguments. Always an object schema with a ``properties``
            map.
    """

    name: str
    description: str
    input_schema: dict[str, object]
