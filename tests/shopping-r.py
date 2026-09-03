from genatp import Agent

agent = Agent(provider="nim", model="stepfun-ai/step-3.5-flash", headless=False)

result = agent.run(
    "Go to a major shopping site, search for a USB-C laptop charger around 65W. "
    "Give me three options with price, rating if visible, shipping estimate if visible, "
    "and why each is a good or bad choice. Do not add anything to cart and do not buy anything.",
    start_url="https://www.amazon.com",
)
print(f"Outcome: {result.outcome}")
print(f"Summary: {result.summary}")
print(f"Tokens: {result.spend.tokens}")
