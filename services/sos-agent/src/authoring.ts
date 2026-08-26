import net from "node:net";
import { Type } from "@earendil-works/pi-ai";
import type { AgentTool } from "@earendil-works/pi-agent-core";

const MAX_RESPONSE_BYTES = 8 * 1024 * 1024;

type AuthoringModule = { id: string; source: string };
type AuthoringParent = { experience_id: string; revision_id: string };
type AuthoringDependency = {
  alias: string;
  experience_id: string;
  revision_id: string;
  export_id: string;
  policy: "locked" | "tracked";
  grant?: { properties?: string[]; events?: string[] };
};
type DerivedCandidate = {
  target_experience_id: string;
  parents: AuthoringParent[];
  request: string;
  rationale: string;
  contract: unknown;
  source: string;
  modules?: AuthoringModule[];
};
type ComposedCandidate = {
  target_experience_id: string;
  dependencies: AuthoringDependency[];
  contract: unknown;
  source: string;
  modules?: AuthoringModule[];
};

type AuthoringAction =
  | { action: "get_experience_context" }
  | { action: "get_derivation_context"; parents: AuthoringParent[] }
  | { action: "get_composition_context"; dependencies: AuthoringDependency[] }
  | { action: "validate_experience"; source: string; modules?: AuthoringModule[] }
  | { action: "submit_experience"; source: string; modules?: AuthoringModule[] }
  | ({ action: "validate_derived_experience" } & DerivedCandidate)
  | ({ action: "submit_derived_experience"; replace_existing?: boolean } & DerivedCandidate)
  | ({ action: "validate_composed_experience" } & ComposedCandidate)
  | ({ action: "submit_composed_experience"; replace_existing?: boolean } & ComposedCandidate);

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
  let derivationPhase: "context" | "validate" | "submit" = "context";
  let validatedDerivation: string | undefined;
  let selectedParents: string | undefined;
  let compositionPhase: "context" | "validate" | "submit" = "context";
  let validatedComposition: string | undefined;
  let selectedDependencies: string | undefined;
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
  const parentParameters = Type.Array(
    Type.Object(
      {
        experience_id: Type.String({ minLength: 1, maxLength: 128 }),
        revision_id: Type.String({ minLength: 64, maxLength: 64 }),
      },
      { additionalProperties: false },
    ),
    { minItems: 1, maxItems: 8 },
  );
  const derivedParameters = {
    target_experience_id: Type.String({ minLength: 1, maxLength: 128 }),
    parents: parentParameters,
    request: Type.String({ minLength: 1, maxLength: 16_384 }),
    rationale: Type.String({ minLength: 1, maxLength: 4_096 }),
    contract: Type.Any(),
    source: Type.String({ minLength: 1, maxLength: 262_144 }),
    modules: moduleParameters,
  };
  const dependencyParameters = Type.Array(
    Type.Object(
      {
        alias: Type.String({ minLength: 1, maxLength: 64 }),
        experience_id: Type.String({ minLength: 1, maxLength: 128 }),
        revision_id: Type.String({ minLength: 64, maxLength: 64 }),
        export_id: Type.String({ minLength: 1, maxLength: 64 }),
        policy: Type.Union([Type.Literal("locked"), Type.Literal("tracked")]),
        grant: Type.Optional(
          Type.Object(
            {
              properties: Type.Optional(Type.Array(Type.String({ maxLength: 64 }))),
              events: Type.Optional(Type.Array(Type.String({ maxLength: 64 }))),
            },
            { additionalProperties: false },
          ),
        ),
      },
      { additionalProperties: false },
    ),
    { minItems: 1, maxItems: 16 },
  );
  const composedParameters = {
    target_experience_id: Type.String({ minLength: 1, maxLength: 128 }),
    dependencies: dependencyParameters,
    contract: Type.Any(),
    source: Type.String({ minLength: 1, maxLength: 262_144 }),
    modules: moduleParameters,
  };
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
    {
      name: "get_derivation_context",
      label: "Read derivation parents",
      description:
        "Read complete source and contracts for an explicit bounded set of exact parent revisions before creating a fork or remix.",
      parameters: Type.Object({ parents: parentParameters }, { additionalProperties: false }),
      executionMode: "sequential",
      async execute(_id, parameters, signal) {
        const { parents } = parameters as { parents: AuthoringParent[] };
        const result = await backend.request(
          { action: "get_derivation_context", parents },
          signal,
        );
        derivationPhase = "validate";
        selectedParents = JSON.stringify(parents);
        validatedDerivation = undefined;
        return toolResult(result);
      },
    },
    {
      name: "validate_derived_experience",
      label: "Validate fork or remix",
      description:
        "Validate a complete self-contained API v4 fork or remix against exact parents, every declared export, bounded viewports, and accessibility appearance states.",
      parameters: Type.Object(derivedParameters, { additionalProperties: false }),
      executionMode: "sequential",
      async execute(_id, parameters, signal) {
        const candidate = parameters as DerivedCandidate;
        if (
          derivationPhase !== "validate" ||
          JSON.stringify(candidate.parents) !== selectedParents
        ) {
          throw new Error(
            "validate_derived_experience must follow get_derivation_context for the same parents",
          );
        }
        const result = await backend.request(
          { action: "validate_derived_experience", ...candidate },
          signal,
        );
        derivationPhase = "submit";
        validatedDerivation = JSON.stringify(candidate);
        return toolResult(result);
      },
    },
    {
      name: "submit_derived_experience",
      label: "Submit fork or remix",
      description:
        "Install and register the exact self-contained fork or remix that passed trusted validation. Existing identities require explicit replacement authority.",
      parameters: Type.Object(
        { ...derivedParameters, replace_existing: Type.Optional(Type.Boolean()) },
        { additionalProperties: false },
      ),
      executionMode: "sequential",
      async execute(_id, parameters, signal) {
        const { replace_existing, ...candidate } = parameters as DerivedCandidate & {
          replace_existing?: boolean;
        };
        if (
          derivationPhase !== "submit" ||
          JSON.stringify(candidate) !== validatedDerivation
        ) {
          throw new Error(
            "submit_derived_experience must exactly match the validated derived candidate",
          );
        }
        const result = await backend.request(
          {
            action: "submit_derived_experience",
            ...candidate,
            ...(replace_existing ? { replace_existing: true } : {}),
          },
          signal,
        );
        derivationPhase = "context";
        selectedParents = undefined;
        validatedDerivation = undefined;
        return toolResult(result);
      },
    },
    {
      name: "get_composition_context",
      label: "Read composition dependencies",
      description:
        "Read the exact source, contract, export, policy, and boundary grant for each selected live dependency before authoring a composed experience.",
      parameters: Type.Object(
        { dependencies: dependencyParameters },
        { additionalProperties: false },
      ),
      executionMode: "sequential",
      async execute(_id, parameters, signal) {
        const { dependencies } = parameters as { dependencies: AuthoringDependency[] };
        const result = await backend.request(
          { action: "get_composition_context", dependencies },
          signal,
        );
        compositionPhase = "validate";
        selectedDependencies = JSON.stringify(dependencies);
        validatedComposition = undefined;
        return toolResult(result);
      },
    },
    {
      name: "validate_composed_experience",
      label: "Validate live composition",
      description:
        "Validate a complete API v4 parent experience and its mounts against exact dependency exports, grants, viewports, and accessibility appearance states.",
      parameters: Type.Object(composedParameters, { additionalProperties: false }),
      executionMode: "sequential",
      async execute(_id, parameters, signal) {
        const candidate = parameters as ComposedCandidate;
        if (
          compositionPhase !== "validate" ||
          JSON.stringify(candidate.dependencies) !== selectedDependencies
        ) {
          throw new Error(
            "validate_composed_experience must follow get_composition_context for the same dependencies",
          );
        }
        const result = await backend.request(
          { action: "validate_composed_experience", ...candidate },
          signal,
        );
        compositionPhase = "submit";
        validatedComposition = JSON.stringify(candidate);
        return toolResult(result);
      },
    },
    {
      name: "submit_composed_experience",
      label: "Submit live composition",
      description:
        "Install, register, and resolve the exact composed experience that passed trusted validation. Existing identities require explicit replacement authority.",
      parameters: Type.Object(
        { ...composedParameters, replace_existing: Type.Optional(Type.Boolean()) },
        { additionalProperties: false },
      ),
      executionMode: "sequential",
      async execute(_id, parameters, signal) {
        const { replace_existing, ...candidate } = parameters as ComposedCandidate & {
          replace_existing?: boolean;
        };
        if (
          compositionPhase !== "submit" ||
          JSON.stringify(candidate) !== validatedComposition
        ) {
          throw new Error(
            "submit_composed_experience must exactly match the validated composed candidate",
          );
        }
        const result = await backend.request(
          {
            action: "submit_composed_experience",
            ...candidate,
            ...(replace_existing ? { replace_existing: true } : {}),
          },
          signal,
        );
        compositionPhase = "context";
        selectedDependencies = undefined;
        validatedComposition = undefined;
        return toolResult(result);
      },
    },
  ];
}
