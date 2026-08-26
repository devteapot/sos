import net from "node:net";
import { Type } from "@earendil-works/pi-ai";
import type { AgentTool } from "@earendil-works/pi-agent-core";

const MAX_RESPONSE_BYTES = 8 * 1024 * 1024;

type AuthoringModule = { id: string; source: string };

type AuthoringAction =
  | { action: "get_experience_context" }
  | { action: "validate_experience"; source: string; modules?: AuthoringModule[] }
  | { action: "submit_experience"; source: string; modules?: AuthoringModule[] };

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
  let phase: "context" | "validate" | "submit" = "context";
  let validatedPackage: string | undefined;
  const moduleParameters = Type.Optional(
    Type.Array(
      Type.Object(
        {
          id: Type.String({ minLength: 3, maxLength: 128 }),
          source: Type.String({ minLength: 1, maxLength: 262_144 }),
        },
        { additionalProperties: false },
      ),
      { maxItems: 16 },
    ),
  );
  return [
    {
      name: "get_experience_context",
      label: "Read active experience",
      description:
        "Read the active SOS Luau experience, revision, and schema. Call this before proposing a change.",
      parameters: Type.Object({}, { additionalProperties: false }),
      executionMode: "sequential",
      async execute(_id, _parameters, signal) {
        const result = await backend.request({ action: "get_experience_context" }, signal);
        phase = "validate";
        validatedPackage = undefined;
        return toolResult(result);
      },
    },
    {
      name: "validate_experience",
      label: "Validate candidate experience",
      description:
        "Request validation of a complete candidate Luau experience package. Optional revision-local modules use namespaced ids such as theme.stock and are loaded with require(id). This stages validation only and never proves activation.",
      parameters: Type.Object(
        {
          source: Type.String({ minLength: 1, maxLength: 262_144 }),
          modules: moduleParameters,
        },
        { additionalProperties: false },
      ),
      executionMode: "sequential",
      async execute(_id, parameters, signal) {
        if (phase !== "validate") {
          throw new Error("validate_experience must follow get_experience_context");
        }
        const { source, modules } = parameters as {
          source: string;
          modules?: AuthoringModule[];
        };
        const result = await backend.request(
          { action: "validate_experience", source, ...(modules ? { modules } : {}) },
          signal,
        );
        if (
          typeof result === "object" &&
          result !== null &&
          "valid" in result &&
          (result as { valid?: unknown }).valid === false
        ) {
          phase = "validate";
          validatedPackage = undefined;
          return toolResult(result);
        }
        phase = "submit";
        validatedPackage = JSON.stringify({ source, modules: modules ?? null });
        return toolResult(result);
      },
    },
    {
      name: "submit_experience",
      label: "Submit candidate experience",
      description:
        "Submit the exact complete source and modules accepted by validate_experience to the trusted host. The trusted host alone may compile, render, and activate them.",
      parameters: Type.Object(
        {
          source: Type.String({ minLength: 1, maxLength: 262_144 }),
          modules: moduleParameters,
        },
        { additionalProperties: false },
      ),
      executionMode: "sequential",
      async execute(_id, parameters, signal) {
        const { source, modules } = parameters as {
          source: string;
          modules?: AuthoringModule[];
        };
        const candidatePackage = JSON.stringify({ source, modules: modules ?? null });
        if (phase !== "submit" || candidatePackage !== validatedPackage) {
          throw new Error(
            "submit_experience source and modules must exactly match the validated candidate",
          );
        }
        const result = await backend.request(
          { action: "submit_experience", source, ...(modules ? { modules } : {}) },
          signal,
        );
        phase = "context";
        validatedPackage = undefined;
        return toolResult(result);
      },
    },
  ];
}
