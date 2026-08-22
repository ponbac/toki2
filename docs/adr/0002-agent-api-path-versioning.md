# Agent API paths stay unversioned; the document version is the contract

Existing clients already call `/time-tracking/timer`. The agent catalog keeps those paths and versions the contract in OpenAPI `info.version` (`1.0.0`) instead of introducing `/v1`. A URL prefix would break the Omarchy widget for no current compatibility need; a breaking change can introduce `/v2` later.
