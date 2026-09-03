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
from collections.abc import Callable
from dataclasses import dataclass
from typing import Final, Literal

from genatp._native import run_agent as _native_run
from genatp._types import BudgetSpend, RunResult
from genatp.errors import RunError

#: Default URL the agent navigates to if the caller does not pass one
#: explicitly. Google is the most common ``"start here"`` for research
#: tasks — chosen for the PRD quickstart, not as a recommendation.
DEFAULT_START_URL: Final[str] = "https://www.google.com"

AskUserFn = Callable[[str, list[str] | None], str]


@dataclass(slots=True)
class Agent:
    """A single agent able to drive a real browser to accomplish tasks.

    The agent is **stateless across successful calls** by default: each
    :meth:`run` launches a fresh browser, runs the plan loop, then tears
    Chrome down. Pass a stable :attr:`user_data_dir` (and use Auth Vault /
    MCP ``save_auth``) to keep sessions across runs. After an interrupt,
    re-run with the same ``user_data_dir`` and a continue-style goal so the
    agent restudies the live page.

    Args:
        model: LLM model id to send on every ``CompletionRequest``.
            If ``None``, the provider's default model is used.
        provider: One of ``"anthropic"``, ``"openai"``, ``"nim"``,
            ``"ollama"``, ``"mock"``. ``None`` auto-detects from env
            vars (see :func:`genatp.resolve_provider_from_env`).
        max_steps: Step ceiling. Independent of token / wall budgets.
        max_tokens: Hard token budget (``0`` = unlimited). Default
            ``100_000`` to prevent NIM context blow-ups.
        max_wall_seconds: Wall-clock budget (``0`` = unlimited). Default
            ``600``.
        max_usd: Dollar budget (``0`` = unlimited).
        headless: Chrome headless mode.
        user_data_dir: Persistent profile path. ``None`` uses a fresh
            temp directory each run (recommended for ephemeral agents).
        system_prompt: Override the orchestrator's default system
            prompt.
        record: When ``True``, successful runs are saved as Action
            Cassettes (any site).
        record_slug: Cassette slug when ``record`` is True (defaults to
            the task id).
        ask_user: Optional ``(question, options|None) -> str`` callback.
            When omitted and ``headless=False``, stdin is used.
    """

    model: str | None = None
    provider: Literal["anthropic", "openai", "nim", "ollama", "mock"] | None = None
    max_steps: int = 30
    max_tokens: int = 100_000
    max_wall_seconds: int = 600
    max_usd: float = 0.0
    headless: bool = True
    user_data_dir: str | None = None
    system_prompt: str | None = None
    mock_reply: str | None = None
    record: bool = False
    record_slug: str | None = None
    ask_user: AskUserFn | None = None

    def run(
        self,
        goal: str,
        *,
        start_url: str | None = None,
        task_id: str | None = None,
        record: bool | None = None,
        record_slug: str | None = None,
    ) -> RunResult:
        """Run the agent against one goal.

        Ctrl+C cooperatively aborts after the current step (headed or
        headless). To continue later, call :meth:`run` again with the same
        :attr:`user_data_dir` and a goal that says to continue from the
        current page.
        """
        start = start_url or DEFAULT_START_URL
        do_record = self.record if record is None else record
        slug = self.record_slug if record_slug is None else record_slug
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
                record=do_record,
                record_slug=slug,
                ask_user=self.ask_user,
                enable_stdin_ask=None,
            )
        except Exception as exc:
            raise RunError(str(exc)) from exc

        spend = BudgetSpend(
            tokens=native.tokens,
            usd=native.usd,
            event_count=native.event_count,
        )
        outcome: Literal["success", "aborted", "error"] = native.outcome  # type: ignore[assignment]
        return RunResult(
            outcome=outcome,
            summary=native.summary,
            steps=native.steps,
            spend=spend,
        )

    def continue_after_interrupt(
        self,
        original_goal: str,
        *,
        start_url: str | None = None,
        task_id: str | None = None,
    ) -> RunResult:
        """Resume after a cooperative interrupt by restudying the page.

        Requires a persistent :attr:`user_data_dir` so the headed browser
        profile (cookies / tabs) still exists. Prefixes the goal so the
        model treats the open page as current state.
        """
        if not self.user_data_dir:
            raise RunError(
                "continue_after_interrupt requires Agent(user_data_dir=...) "
                "so the browser profile survives the interrupt"
            )
        cont = (
            "Continue the previous task after a user interrupt. "
            "Restudy the CURRENT page snapshot carefully before acting. "
            f"Original goal: {original_goal}"
        )
        return self.run(cont, start_url=start_url or DEFAULT_START_URL, task_id=task_id)

    async def arun(
        self,
        goal: str,
        *,
        start_url: str | None = None,
        task_id: str | None = None,
        record: bool | None = None,
        record_slug: str | None = None,
    ) -> RunResult:
        """Async wrapper for :meth:`run`."""
        return await asyncio.to_thread(
            self.run,
            goal,
            start_url=start_url,
            task_id=task_id,
            record=record,
            record_slug=record_slug,
        )
