export const MAX_PROMPT_BYTES = 32 * 1024;
export const MAX_SOURCE_BYTES = 256 * 1024;
export const MAX_STDIO_REQUEST_BYTES = 1024 * 1024;

export function isBoundedPrompt(value: unknown): value is string {
  return (
    typeof value === "string" &&
    Boolean(value.trim()) &&
    Buffer.byteLength(value) <= MAX_PROMPT_BYTES
  );
}

export function isBoundedSource(value: unknown): value is string {
  return (
    typeof value === "string" &&
    Boolean(value) &&
    Buffer.byteLength(value) <= MAX_SOURCE_BYTES
  );
}
