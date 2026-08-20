import { HttpsProxyAgent } from "https-proxy-agent";
import type { CoreDevProxyHooks } from "./stdio-runner.js";
import {
  nodeProviderFetchWithOptions,
  type NodeFetchBackend,
} from "./provider-fetch.js";

export const CORE_DEV_OPENROUTER_PROXY = "http://127.0.0.1:37173";

export function fixedCoreDevProxyFetch(
  input: Parameters<typeof globalThis.fetch>[0],
  init: Parameters<typeof globalThis.fetch>[1],
  fetchImplementation?: NodeFetchBackend,
): Promise<Response> {
  const url = new URL(input instanceof Request ? input.url : input.toString());
  if (
    url.protocol !== "https:" ||
    url.hostname !== "openrouter.ai" ||
    (url.port !== "" && url.port !== "443")
  ) {
    throw new Error("invalid Pi runner request");
  }
  const agent = new HttpsProxyAgent(CORE_DEV_OPENROUTER_PROXY);
  return nodeProviderFetchWithOptions(
    input,
    init,
    { agent, redirect: "error" },
    fetchImplementation,
  );
}

export const CORE_DEV_PROXY_HOOKS: CoreDevProxyHooks = {
  accepts(value): value is string {
    return value === CORE_DEV_OPENROUTER_PROXY;
  },
  fetch(proxy) {
    if (proxy !== CORE_DEV_OPENROUTER_PROXY) throw new Error("invalid Pi runner request");
    return (input, init) => fixedCoreDevProxyFetch(input, init);
  },
};
