// Runtime contract bindings: the schemas in contracts/schemas/ are imported
// once here so the gateway and the tests enforce the same files.

import envelopeSchema from "../../contracts/schemas/event-envelope.schema.json";
import commandSchema from "../../contracts/schemas/client-command.schema.json";
import inferenceRequestSchema from "../../contracts/schemas/inference-request.schema.json";
import inferenceEventSchema from "../../contracts/schemas/inference-event.schema.json";
import toolProposalSchema from "../../contracts/schemas/tool-proposal.schema.json";
import permissionGrantSchema from "../../contracts/schemas/permission-grant.schema.json";
import memoryRecordSchema from "../../contracts/schemas/memory-record.schema.json";

export const SCHEMAS = {
  eventEnvelope: envelopeSchema as unknown as Record<string, unknown>,
  clientCommand: commandSchema as unknown as Record<string, unknown>,
  inferenceRequest: inferenceRequestSchema as unknown as Record<string, unknown>,
  inferenceEvent: inferenceEventSchema as unknown as Record<string, unknown>,
  toolProposal: toolProposalSchema as unknown as Record<string, unknown>,
  permissionGrant: permissionGrantSchema as unknown as Record<string, unknown>,
  memoryRecord: memoryRecordSchema as unknown as Record<string, unknown>,
} as const;
