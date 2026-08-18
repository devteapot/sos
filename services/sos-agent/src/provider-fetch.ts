import { Readable } from "node:stream";
import nodeFetch, {
  type RequestInit as NodeFetchRequestInit,
  type Response as NodeFetchResponse,
} from "node-fetch";

export type NodeFetchBackend = (
  url: string,
  init?: NodeFetchRequestInit,
) => Promise<NodeFetchResponse>;

async function normalizeRequest(
  input: Parameters<typeof globalThis.fetch>[0],
  init: Parameters<typeof globalThis.fetch>[1],
): Promise<{ url: string; init: NodeFetchRequestInit }> {
  const request = input instanceof Request ? input : undefined;
  const url = request ? request.url : input.toString();
  const inheritedBody =
    request && request.method !== "GET" && request.method !== "HEAD"
      ? Buffer.from(await request.clone().arrayBuffer())
      : undefined;
  const normalized = {
      ...(request
        ? {
            method: request.method,
            headers: request.headers as unknown as NodeFetchRequestInit["headers"],
            signal: request.signal,
            ...(inheritedBody ? { body: inheritedBody } : {}),
          }
        : {}),
      ...(init as unknown as NodeFetchRequestInit),
  } as unknown as NodeFetchRequestInit;
  return { url, init: normalized };
}

function toWebResponse(response: NodeFetchResponse): Response {
  const body = response.body
    ? (Readable.toWeb(response.body as Readable) as ReadableStream<Uint8Array>)
    : null;
  return new Response(body, {
    status: response.status,
    statusText: response.statusText,
    headers: [...response.headers.entries()],
  });
}

export async function nodeProviderFetchWithOptions(
  input: Parameters<typeof globalThis.fetch>[0],
  init: Parameters<typeof globalThis.fetch>[1],
  options: NodeFetchRequestInit,
  fetchImplementation: NodeFetchBackend = nodeFetch,
): Promise<Response> {
  const request = await normalizeRequest(input, init);
  return toWebResponse(
    await fetchImplementation(request.url, { ...request.init, ...options }),
  );
}

export function nodeProviderFetch(
  input: Parameters<typeof globalThis.fetch>[0],
  init?: Parameters<typeof globalThis.fetch>[1],
): Promise<Response> {
  return nodeProviderFetchWithOptions(input, init, {});
}
