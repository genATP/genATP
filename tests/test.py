from genatp import Agent

agent = Agent(provider="nim", model="stepfun-ai/step-3.5-flash", headless=False)

result = agent.run(
    "go throught the dom of this site and find the list of dropout student(2025-26) links in the webiste somewhere(it might be present in some form of downloadable exccel file.) the login id is 27131101401 and the password is Mal@01401. there is a capta field while login ill fill the capta manually, please wait for 5 seconds on the login page. output the list of those students",
    start_url="https://sdms.udiseplus.gov.in/g0/#/school/4582580/reportingModule",
)
print(f"Outcome: {result.outcome}")
print(f"Summary: {result.summary}")
print(f"Tokens: {result.spend.tokens}")
print(f"{result}")
