# The OpenAPI catalog is the agent contract seam

Runtime handlers are registered once in the normal Axum routers, and the protected route tree accepts either browser sessions or bearer tokens. The `AgentApi` OpenAPI derive separately lists the operations published to agents. This keeps authentication independent from agent discovery while retaining an explicit, test-covered catalog allowlist. Omitting a route from the document does not prevent a bearer-authenticated caller from invoking it directly.
