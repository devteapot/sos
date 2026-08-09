import fs from "node:fs/promises";
import nodePath from "node:path";
import process from "node:process";
import type {
  AuthOperationOptions,
  Credential,
  CredentialInfo,
  CredentialStore,
} from "@earendil-works/pi-ai";

const LOCK_RETRY_MS = 50;
const LOCK_TIMEOUT_MS = 30_000;
const ABANDONED_LOCK_MS = 5 * 60_000;

type CredentialDocument = Record<string, Credential>;

function abortError(signal: AbortSignal): Error {
  return signal.reason instanceof Error ? signal.reason : new Error("credential operation aborted");
}

async function pause(milliseconds: number, signal?: AbortSignal): Promise<void> {
  if (signal?.aborted) throw abortError(signal);
  await new Promise<void>((resolve, reject) => {
    const finish = () => {
      signal?.removeEventListener("abort", abort);
      resolve();
    };
    const timer = setTimeout(finish, milliseconds);
    const abort = () => {
      clearTimeout(timer);
      signal?.removeEventListener("abort", abort);
      reject(abortError(signal as AbortSignal));
    };
    signal?.addEventListener("abort", abort, { once: true });
  });
}

function isCredential(value: unknown): value is Credential {
  if (!value || typeof value !== "object") return false;
  const candidate = value as Record<string, unknown>;
  if (candidate.type === "api_key") {
    return candidate.key === undefined || typeof candidate.key === "string";
  }
  return (
    candidate.type === "oauth" &&
    typeof candidate.access === "string" &&
    typeof candidate.refresh === "string" &&
    typeof candidate.expires === "number" &&
    Number.isFinite(candidate.expires)
  );
}

function decodeDocument(raw: string, source: string): CredentialDocument {
  const decoded: unknown = JSON.parse(raw);
  if (!decoded || typeof decoded !== "object" || Array.isArray(decoded)) {
    throw new Error(`credential store ${source} must contain a JSON object`);
  }
  for (const [provider, credential] of Object.entries(decoded)) {
    if (!provider || !isCredential(credential)) {
      throw new Error(`credential store ${source} has an invalid entry for ${provider || "<empty>"}`);
    }
  }
  return decoded as CredentialDocument;
}

/**
 * File-backed Pi credential storage with atomic replacement and a process-safe
 * mutation lock. Pi owns login, refresh, and request authentication; SOS owns
 * only the durable storage boundary required by CredentialStore.
 */
export class JsonCredentialStore implements CredentialStore {
  private mutationTail: Promise<void> = Promise.resolve();

  constructor(readonly path: string) {}

  async read(providerId: string, options?: AuthOperationOptions): Promise<Credential | undefined> {
    options?.signal?.throwIfAborted();
    return (await this.readDocument())[providerId];
  }

  async list(options?: AuthOperationOptions): Promise<readonly CredentialInfo[]> {
    options?.signal?.throwIfAborted();
    return Object.entries(await this.readDocument()).map(([providerId, credential]) => ({
      providerId,
      type: credential.type,
    }));
  }

  modify(
    providerId: string,
    fn: (current: Credential | undefined) => Promise<Credential | undefined>,
    options?: AuthOperationOptions,
  ): Promise<Credential | undefined> {
    return this.enqueue(async () =>
      this.withLock(async () => {
        options?.signal?.throwIfAborted();
        const document = await this.readDocument();
        const current = document[providerId];
        const next = await fn(current);
        options?.signal?.throwIfAborted();
        if (next !== undefined) {
          document[providerId] = next;
          await this.writeDocument(document);
          return next;
        }
        return current;
      }, options?.signal),
    );
  }

  delete(providerId: string, options?: AuthOperationOptions): Promise<void> {
    return this.enqueue(async () =>
      this.withLock(async () => {
        options?.signal?.throwIfAborted();
        const document = await this.readDocument();
        if (!(providerId in document)) return;
        delete document[providerId];
        await this.writeDocument(document);
      }, options?.signal),
    );
  }

  private enqueue<T>(operation: () => Promise<T>): Promise<T> {
    const queued = this.mutationTail.catch(() => undefined).then(operation);
    this.mutationTail = queued.then(
      () => undefined,
      () => undefined,
    );
    return queued;
  }

  private async readDocument(): Promise<CredentialDocument> {
    try {
      const metadata = await fs.lstat(this.path);
      if (!metadata.isFile()) throw new Error(`credential store ${this.path} is not a regular file`);
      return decodeDocument(await fs.readFile(this.path, "utf8"), this.path);
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === "ENOENT") return {};
      throw error;
    }
  }

  private async writeDocument(document: CredentialDocument): Promise<void> {
    const directory = nodePath.dirname(this.path);
    await fs.mkdir(directory, { recursive: true, mode: 0o700 });
    const temporary = `${this.path}.tmp-${process.pid}-${Date.now()}`;
    try {
      await fs.writeFile(temporary, `${JSON.stringify(document)}\n`, {
        encoding: "utf8",
        flag: "wx",
        mode: 0o600,
      });
      await fs.rename(temporary, this.path);
    } finally {
      await fs.unlink(temporary).catch((error: NodeJS.ErrnoException) => {
        if (error.code !== "ENOENT") throw error;
      });
    }
  }

  private async withLock<T>(operation: () => Promise<T>, signal?: AbortSignal): Promise<T> {
    const lockPath = `${this.path}.lock`;
    await fs.mkdir(nodePath.dirname(this.path), { recursive: true, mode: 0o700 });
    const deadline = Date.now() + LOCK_TIMEOUT_MS;
    for (;;) {
      signal?.throwIfAborted();
      try {
        await fs.mkdir(lockPath, { mode: 0o700 });
        await fs.writeFile(nodePath.join(lockPath, "owner"), `${process.pid}\n`, { mode: 0o600 });
        break;
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== "EEXIST") throw error;
        if (await this.reapAbandonedLock(lockPath)) continue;
        if (Date.now() >= deadline) throw new Error(`timed out waiting for credential lock ${lockPath}`);
        await pause(LOCK_RETRY_MS, signal);
      }
    }
    try {
      return await operation();
    } finally {
      await fs.rm(lockPath, { recursive: true, force: true });
    }
  }

  private async reapAbandonedLock(lockPath: string): Promise<boolean> {
    try {
      const owner = Number.parseInt(
        (await fs.readFile(nodePath.join(lockPath, "owner"), "utf8")).trim(),
        10,
      );
      if (Number.isInteger(owner) && owner > 0) {
        try {
          process.kill(owner, 0);
          return false;
        } catch (error) {
          if ((error as NodeJS.ErrnoException).code !== "ESRCH") return false;
        }
      } else {
        return false;
      }
      await fs.rm(lockPath, { recursive: true, force: true });
      return true;
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === "ENOENT") {
        try {
          const metadata = await fs.stat(lockPath);
          if (Date.now() - metadata.mtimeMs < ABANDONED_LOCK_MS) return false;
          await fs.rm(lockPath, { recursive: true, force: true });
          return true;
        } catch (nested) {
          if ((nested as NodeJS.ErrnoException).code === "ENOENT") return true;
          throw nested;
        }
      }
      throw error;
    }
  }
}
