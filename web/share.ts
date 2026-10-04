// Share links: a tune, deflated and base64url-encoded, in the URL's hash.
//
//   https://…/#n=Ode%20to%20Joy&t=<deflate-raw, base64url>

export interface Shared {
  name: string | null;
  source: string;
}

async function deflate(text: string): Promise<Uint8Array> {
  const stream = new Blob([text]).stream().pipeThrough(new CompressionStream("deflate-raw"));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

async function inflate(bytes: Uint8Array<ArrayBuffer>): Promise<string> {
  const stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream("deflate-raw"));
  return new Response(stream).text();
}

function toBase64Url(bytes: Uint8Array): string {
  let bin = "";
  for (const b of bytes) bin += String.fromCharCode(b);
  return btoa(bin).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function fromBase64Url(text: string): Uint8Array<ArrayBuffer> {
  return Uint8Array.from(atob(text.replace(/-/g, "+").replace(/_/g, "/")), (c) => c.charCodeAt(0));
}

/** The hash (with its `#`) that shares `source` under `name`. */
export async function shareHash(source: string, name: string): Promise<string> {
  const params = new URLSearchParams({ n: name, t: toBase64Url(await deflate(source)) });
  return `#${params}`;
}

/** The tune a hash shares, or null if it shares none. Throws if it is damaged. */
export async function readShareHash(hash: string): Promise<Shared | null> {
  const params = new URLSearchParams(hash.replace(/^#/, ""));
  const t = params.get("t");
  if (t === null) return null;
  return { name: params.get("n"), source: await inflate(fromBase64Url(t)) };
}
