from genatp import Agent

agent = Agent(provider="nim", model="stepfun-ai/step-3.5-flash", headless=False)

result = agent.run(
    "Compare the pricing pages for Notion and Todoist for an individual user. "
    "Make a table with free-plan limits, cheapest paid monthly price, and "
    "one reason I might choose each. Do not sign up or click checkout.",
    start_url="https://www.notion.so/pricing",
)
print(f"Outcome: {result.outcome}")
print(f"Summary: {result.summary}")
print(f"Tokens: {result.spend.tokens}")
