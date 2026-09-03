from genatp import Agent

agent = Agent(provider="nim", model="stepfun-ai/step-3.5-flash", headless=False)

result = agent.run(
    "Fill this contact form with the following test data: name Alex Tester, "
    "email alex.tester@example.com, subject Product question, message "
    "'Please send more information about your API.' Stop before submitting "
    "and tell me what you filled.",
    start_url="https://www.automationtesting.co.uk/contactForm.html",
)

print(f"Outcome: {result.outcome}")
print(f"Summary: {result.summary}")
print(f"Tokens: {result.spend.tokens}")
