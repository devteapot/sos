import net from "node:net";
import { Type } from "@earendil-works/pi-ai";
import type { AgentTool } from "@earendil-works/pi-agent-core";

const MAX_RESPONSE_BYTES = 1024 * 1024;

type AuthoringAction =
  | { action: "get_experience_context" }
  | { action: "validate_experience"; source: string }
  | { action: "submit_experience"; source: string };

interface AuthoringResponse {
  ok: boolean;
  result?: unknown;
  error?: string;
}

export interface AuthoringBackend {
  request(action: AuthoringAction, signal?: AbortSignal): Promise<unknown>;
}

export class UnixAuthoringBackend implements AuthoringBackend {
  constructor(private readonly socketPath: string) {}

  request(action: AuthoringAction, signal?: AbortSignal): Promise<unknown> {
    return new Promise((resolve, reject) => {
      const socket = net.createConnection(this.socketPath);
      let bytes = 0;
      let response = "";
      const abort = () => socket.destroy(new Error("authoring request aborted"));
      signal?.addEventListener("abort", abort, { once: true });
      const finish = () => signal?.removeEventListener("abort", abort);
      socket.setEncoding("utf8");
      socket.setTimeout(30_000, () => socket.destroy(new Error("authoring broker timed out")));
      socket.on("connect", () => socket.end(`${JSON.stringify(action)}\n`));
      socket.on("data", (chunk: string) => {
        bytes += Buffer.byteLength(chunk);
        if (bytes > MAX_RESPONSE_BYTES) {
          socket.destroy(new Error("authoring response is too large"));
          return;
        }
        response += chunk;
      });
      socket.on("error", (error) => {
        finish();
        reject(error);
      });
      socket.on("end", () => {
        finish();
        try {
          const decoded = JSON.parse(response.trim()) as AuthoringResponse;
          if (!decoded.ok) throw new Error(decoded.error ?? "authoring request failed");
          resolve(decoded.result);
        } catch (error) {
          reject(error);
        }
      });
    });
  }
}

function toolResult(result: unknown) {
  return {
    content: [{ type: "text" as const, text: JSON.stringify(result) }],
    details: result,
  };
}

export function createAuthoringTools(backend: AuthoringBackend): AgentTool[] {
  return [
    {
      name: "get_experience_context",
      label: "Read active experience",
      description:
        "Read the active SOS Luau experience, revision, and schema. Call this before proposing a change.",
      parameters: Type.Object({}, { additionalProperties: false }),
      executionMode: "sequential",
      async execute(_id, _parameters, signal) {
        return toolResult(await backend.request({ action: "get_experience_context" }, signal));
      },
    },
    {
      name: "validate_experience",
      label: "Validate candidate experience",
      description:
        "Compile, migrate, and render a complete candidate Luau experience against the deterministic SOS provider snapshot. This does not activate it.",
      parameters: Type.Object(
        { source: Type.String({ minLength: 1, maxLength: 262_144 }) },
        { additionalProperties: false },
      ),
      executionMode: "sequential",
      async execute(_id, parameters, signal) {
        const source = (parameters as { source: string }).source;
        return toolResult(
          await backend.request({ action: "validate_experience", source }, signal),
        );
      },
    },
    {
      name: "submit_experience",
      label: "Activate candidate experience",
      description:
        "Validate, install, stage, and transactionally activate a complete candidate Luau experience. Use only after validate_experience succeeds with the exact same source.",
      parameters: Type.Object(
        { source: Type.String({ minLength: 1, maxLength: 262_144 }) },
        { additionalProperties: false },
      ),
      executionMode: "sequential",
      async execute(_id, parameters, signal) {
        const source = (parameters as { source: string }).source;
        return toolResult(
          await backend.request({ action: "submit_experience", source }, signal),
        );
      },
    },
  ];
}
