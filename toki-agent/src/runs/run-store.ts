import type { AgentRunRecord } from "../domain/schemas";

export interface RunStore {
  readonly get: (id: string) => Promise<AgentRunRecord | undefined>;
  readonly put: (record: AgentRunRecord) => Promise<void>;
}
