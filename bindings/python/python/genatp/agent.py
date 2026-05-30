"""High-level :class:`Agent` facade — the public face of the SDK.

The class is intentionally tiny: every method just normalises args and
dispatches into :mod:`genatp._native`. Heavy lifting (LLM dance,
budget, retries, perception) stays in the Rust workspace.

PRD 5-line quickstart::

    from genatp import Agent

    agent = Agent(model="claude-3-5-sonnet-latest")
    result = agent.run("Find the cheapest direct flight from SFO to JFK next Friday.")
    print(result.summary)
"""

from __future__ import annotations

import asyncio
from dataclasses import dataclass
from typing import Final, Literal

from genatp._native import run_agent as _native_run
from genatp._types import BudgetSpend, RunResult
from genatp.errors import RunError

#: Default URL the agent navigates to if the caller does not pass one
#: explicitly. Google is the most common ``"start here"`` for research
#: tasks — chosen for the PRD quickstart, not as a recommendation.
DEFAULT_START_URL: Final[str] = "https://www.google.com"


@dataclass(slots=True)
class Agent:
    """A single agent able to drive a real browser to accomplish tasks.

    The agent is **stateless across calls**: each :meth:`run` launches
    a fresh browser, runs the plan loop, then tears Chrome down. For
    longer interactive sessions, use :class:`genatp.BrowserSession`
    directly.

    Args:
        model: LLM model id to send on every ``CompletionRequest``.
            If ``None``, the provider's default model is used.
        provider: One of ``"anthropic"``, ``"openai"``, ``"nim"``,
            ``"ollama"``, ``"mock"``. ``None`` auto-detects from env
            vars (see :func:`genatp.resolve_provider_from_env`).
        max_steps: Step ceiling. Independent of token / wall budgets.
        max_tokens: Hard token budget (0 = unlimited).
        max_wall_seconds: Wall-clock budget (0 = unlimited).
        max_usd: Dollar budget (0 = unlimited; only meaningful when
            the orchestrator's :class:`Pricing` is non-zero, which is
            today a TODO for the Python wrapper — see issue tracker).
        headless: Chrome headless mode.
        user_data_dir: Persistent profile path. ``None`` uses a fresh
            temp directory each run (recommended for ephemeral agents).
        system_prompt: Override the orchestrator's default system
            prompt. Use this to scope the agent (``"You only answer
            questions about company internals."``).

    Examples:
        Mock-provider smoke test (no network)::

            agent = Agent(provider="mock", mock_reply="done")
            r = agent.run("Greet me", start_url="data:text/html,<h1>hi</h1>")
            assert r.outcome == "success"
    """

    model: str | None = None
    provider: Literal["anthropic", "openai", "nim", "ollama", "mock"] | None = None
    max_steps: int = 30
    max_tokens: int = 0
    max_wall_seconds: int = 0
    max_usd: float = 0.0
    headless: bool = True
    user_data_dir: str | None = None
    system_prompt: str | None = None
    mock_reply: str | None = None

    def run(
        self,
        goal: str,
        *,
        start_url: str | None = None,
        task_id: str | None = None,
    ) -> RunResult:
        """Run the agent against one goal.

        Blocks the calling thread until the orchestrator returns. Use
        :meth:`arun` from inside an ``asyncio`` loop instead — it
        offloads the blocking work to :func:`asyncio.to_thread`.

        Args:
            goal: Plain-English task description.
            start_url: URL the agent navigates to first. Defaults to
                :data:`DEFAULT_START_URL`.
            task_id: Caller-supplied id used as the event-log filename
                in long-lived runs. Defaults to a timestamp-based id.

        Returns:
            :class:`genatp.RunResult` describing the outcome.

        Raises:
            RunError: For unexpected errors that escape the orchestrator
                (typically misconfiguration). Note that ``aborted`` is a
                *normal* outcome and surfaces through
                :attr:`RunResult.outcome`, not via this exception.
        """
        start = start_url or DEFAULT_START_URL
        try:
            native = _native_run(
                goal=goal,
                start_url=start,
                model=self.model,
                max_steps=self.max_steps,
                max_tokens=self.max_tokens,
                max_wall_s=self.max_wall_seconds,
                max_usd=self.max_usd,
                system_prompt=self.system_prompt,
                task_id=task_id,
                headless=self.headless,
                user_data_dir=self.user_data_dir,
                provider=self.provider,
                mock_reply=self.mock_reply,
            )
        except Exception as exc:  # noqa: BLE001 — PyO3 RuntimeError
            raise RunError(str(exc)) from exc

        spend = BudgetSpend(
            tokens=native.tokens,
            usd=native.usd,
            event_count=native.event_count,
        )
        # The Rust layer guarantees `outcome` is one of three known strings.
        outcome: Literal["success", "aborted", "error"] = native.outcome  # type: ignore[assignment]
        return RunResult(
            outcome=outcome,
            summary=native.summary,
            steps=native.steps,
            spend=spend,
        )

    async def arun(
        self,
        goal: str,
        *,
        start_url: str | None = None,
        task_id: str | None = None,
    ) -> RunResult:
        """Async wrapper for :meth:`run`.

        Implemented via :func:`asyncio.to_thread` so the calling event
        loop stays unblocked while Chrome and the LLM finish their
        roundtrips. The signature mirrors :meth:`run`.
        """
        return await asyncio.to_thread(self.run, goal, start_url=start_url, task_id=task_id)
