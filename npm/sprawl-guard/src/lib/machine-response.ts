import { Result } from "better-result";
import { z } from "zod/v4";
import { RuntimeFailureError } from "./errors.js";
import {
  MachineResponseResultKind,
  MachineCurrentProtocolVersionSchema,
  MachineOperationalErrorExitCodeSchema,
  MachineProcessExitCodeSchema,
  MachineRuntimeErrorCodeSchema,
} from "./machine-contract.js";

const MachineRequestIssueSchema = z
  .object({
    message: z.string(),
  })
  .strict();

const MachineCompletedResponseSchema = z
  .object({
    kind: z.literal(MachineResponseResultKind.Completed),
    exit_code: MachineProcessExitCodeSchema,
    stdout: z.string(),
    stderr: z.string(),
    payload: z.unknown().optional(),
  })
  .strict();

const MachineRequestErrorResponseSchema = z
  .object({
    kind: z.literal(MachineResponseResultKind.RequestError),
    exit_code: MachineOperationalErrorExitCodeSchema,
    stdout: z.string(),
    stderr: z.string(),
    issues: z.array(MachineRequestIssueSchema).nonempty(),
  })
  .strict();

const MachineRuntimeErrorResponseSchema = z
  .object({
    kind: z.literal(MachineResponseResultKind.RuntimeError),
    exit_code: MachineOperationalErrorExitCodeSchema,
    stdout: z.string(),
    stderr: z.string(),
    error_code: MachineRuntimeErrorCodeSchema,
  })
  .strict();

const MachineResponseEnvelopeSchema = z
  .object({
    protocol_version: MachineCurrentProtocolVersionSchema,
    result: z.discriminatedUnion("kind", [
      MachineCompletedResponseSchema,
      MachineRequestErrorResponseSchema,
      MachineRuntimeErrorResponseSchema,
    ]),
  })
  .strict();

type MachineCompletedResponse = z.infer<typeof MachineCompletedResponseSchema>;
type MachineRequestErrorResponse = z.infer<typeof MachineRequestErrorResponseSchema>;
type MachineRuntimeErrorResponse = z.infer<typeof MachineRuntimeErrorResponseSchema>;

export type MachineResponseResult =
  | MachineCompletedResponse
  | MachineRequestErrorResponse
  | MachineRuntimeErrorResponse;

export function parseMachineResponse(
  rawResponse: string,
): Result<MachineResponseResult, RuntimeFailureError> {
  const parsed = Result.try({
    try: () => JSON.parse(rawResponse) as unknown,
    catch: (cause) =>
      new RuntimeFailureError(
        "parse machine response",
        "failed to parse machine response JSON",
        cause,
      ),
  });
  if (Result.isError(parsed)) {
    return parsed;
  }

  const response = MachineResponseEnvelopeSchema.safeParse(parsed.value);
  if (!response.success) {
    return Result.err(
      new RuntimeFailureError(
        "parse machine response",
        "machine response does not match the supported machine contract",
        response.error,
      ),
    );
  }

  return Result.ok(response.data.result);
}
