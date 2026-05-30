"""End-to-end smoke test that hits no network, no LLM, no real site.

Uses the bundled `mock` provider — the orchestrator treats its text-only
response as a successful finish, so this exercises the full plan loop
including browser startup, snapshot capture, and event-log
hash-chaining.

Run::

    python examples/mock_smoke.py
"""

from __future__ import annotations

from genatp import Agent


def main() -> None:
    agent = Agent(provider="mock", mock_reply="all done", max_steps=2)
    result = agent.run(
        "Greet the user and finish.",
        start_url="data:text/html,<h1>hi from genATP</h1>",
    )
    print(f"outcome={result.outcome} steps={result.steps} summary={result.summary!r}")
    print(f"event_count={result.spend.event_count}")


if __name__ == "__main__":
    main()
