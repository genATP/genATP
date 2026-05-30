"""Wire the genATP MCP tools into a LangChain / LangGraph ReAct agent.

Run::

    pip install genatp[langchain] langgraph langchain-anthropic
    export ANTHROPIC_API_KEY=...
    python examples/langchain_react_agent.py
"""

from __future__ import annotations

from genatp.langchain import as_langchain_tools


def main() -> None:
    try:
        from langchain_anthropic import ChatAnthropic
        from langgraph.prebuilt import create_react_agent
    except ImportError as exc:
        raise SystemExit(
            "Missing optional deps for this example. Install with:\n"
            "  pip install langgraph langchain-anthropic"
        ) from exc

    tools = as_langchain_tools()
    session = tools[0].metadata["session"]  # the shared browser
    try:
        llm = ChatAnthropic(model="claude-3-5-sonnet-latest", temperature=0)
        agent = create_react_agent(llm, tools=tools)
        result = agent.invoke(
            {
                "messages": [
                    (
                        "user",
                        "Open example.com, take a snapshot, and tell me the page title.",
                    )
                ]
            }
        )
        for msg in result["messages"]:
            print(msg)
    finally:
        session.close()


if __name__ == "__main__":
    main()
