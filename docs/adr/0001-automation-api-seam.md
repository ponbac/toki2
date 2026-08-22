# Automation router is the agent contract seam

The Automation API is one Axum `OpenApiRouter`: registering a handler both documents it and makes it globally eligible for bearer tokens. A separate allowlist, or duplicate documented and undocumented copies of the same route, would drift. Per-token capabilities can later narrow that global set; they do not replace this router as the catalog.
