import fs from "node:fs/promises";

const MAX_PROMPT_DOCUMENT_BYTES = 1024 * 1024;

export interface PromptDocuments {
  apiPath: string;
  examples: string[];
}

export async function readSystemPrompt(documents: PromptDocuments): Promise<string> {
  const files = [documents.apiPath, ...documents.examples];
  const sizes = await Promise.all(files.map(async (file) => (await fs.stat(file)).size));
  if (
    sizes.some((size) => size <= 0) ||
    sizes.reduce((total, size) => total + size, 0) > MAX_PROMPT_DOCUMENT_BYTES
  ) {
    throw new Error("the SOS agent prompt documents are outside the bounded size");
  }
  const contents = await Promise.all(files.map((file) => fs.readFile(file, "utf8")));
  return buildSystemPrompt(contents[0] ?? "", contents.slice(1));
}

export function buildSystemPrompt(apiDocument: string, examples: string[]): string {
  if (!apiDocument || examples.length === 0 || examples.some((example) => !example)) {
    throw new Error("the SOS agent prompt documents must not be empty");
  }
  const actualBytes = [apiDocument, ...examples].reduce(
    (total, document) => total + Buffer.byteLength(document),
    0,
  );
  if (actualBytes > MAX_PROMPT_DOCUMENT_BYTES) {
    throw new Error("the SOS agent prompt documents are outside the bounded size");
  }
  return `You are the resident SOS experience author. You modify the currently running visual experience in response to the user's direct request.

Rules:
- Always call get_experience_context first.
- Return a complete Luau experience package, never a patch. Keep the entry source focused and use namespaced revision-local modules for substantial reusable sections or themes.
- Call validate_experience before submit_experience.
- Submit only the exact source and modules that validated.
- The model only proposes and submits a candidate. The trusted host independently compiles, renders, validates, and activates the exact submitted source; never claim activation unless submit_experience's trusted-host response confirms it.
- You have no shell, filesystem, process, or general network tools.
- Preserve the user's current intent and durable state unless they ask for a reset.
- Every revision must keep a visible Luau-authored agent conversation/composer that renders model.agent and emits agent.prompt. You may redesign and reposition it, but never replace it with a native widget or remove the user's way to request another change.

SOS experience API:
${apiDocument}

Reference experiences:
${examples.join("\n\n---\n\n")}`;
}
