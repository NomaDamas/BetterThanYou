export interface Env {
  SHARES: KVNamespace;
  PUBLISH_TOKEN: string;
}

interface ShareMetadata {
  contentType: string;
  uploadedAt: string;
  originalFilename: string;
}

const MAX_UPLOAD_BYTES = 5 * 1024 * 1024;
const ID_ALPHABET = "abcdefghijklmnopqrstuvwxyz0123456789";
const ID_WITH_EXT_RE = /^[a-z0-9]{10}\.(html|png|json)$/;
const SHARE_TTL_SECONDS = 60 * 60 * 24 * 90;
const CORS_HEADERS = {
  "Access-Control-Allow-Origin": "*",
};

const KIND_META = {
  html: { ext: "html", contentType: "text/html; charset=utf-8" },
  png: { ext: "png", contentType: "image/png" },
  json: { ext: "json", contentType: "application/json; charset=utf-8" },
} as const;

type ShareKind = keyof typeof KIND_META;

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    if (request.method === "OPTIONS") {
      return optionsResponse();
    }

    const url = new URL(request.url);

    // The Worker is now hosted on its own subdomain
    // (better-than-you.nomadamas.org), so paths are bare — no `/btyu/`
    // prefix needed. The KV objects are still keyed `share/<id>.<ext>`.
    if (request.method === "GET" && url.pathname === "/healthz") {
      return new Response("ok", {
        status: 200,
        headers: { ...CORS_HEADERS, "Content-Type": "text/plain; charset=utf-8" },
      });
    }

    if (request.method === "POST" && url.pathname === "/share") {
      return uploadShare(request, env, url);
    }

    if (request.method === "GET" && url.pathname.startsWith("/s/")) {
      return getShare(env, url.pathname.slice("/s/".length));
    }

    return new Response("not found", {
      status: 404,
      headers: { ...CORS_HEADERS, "Content-Type": "text/plain; charset=utf-8" },
    });
  },
};

async function uploadShare(request: Request, env: Env, url: URL): Promise<Response> {
  if (!env.PUBLISH_TOKEN || request.headers.get("authorization") !== `Bearer ${env.PUBLISH_TOKEN}`) {
    return jsonResponse({ error: "unauthorized" }, 401);
  }

  const kindParam = url.searchParams.get("kind") ?? "html";
  if (!isShareKind(kindParam)) {
    return jsonResponse({ error: "unsupported kind" }, 400);
  }

  const contentLength = request.headers.get("content-length");
  if (contentLength) {
    const length = Number(contentLength);
    if (!Number.isFinite(length) || length > MAX_UPLOAD_BYTES) {
      return jsonResponse({ error: "payload too large" }, 413);
    }
  }

  const bytes = await request.arrayBuffer();
  if (bytes.byteLength > MAX_UPLOAD_BYTES) {
    return jsonResponse({ error: "payload too large" }, 413);
  }

  const meta = KIND_META[kindParam];
  const id = randomId(10);
  const objectKey = `share/${id}.${meta.ext}`;
  const originalFilename = url.searchParams.get("filename") ?? "";

  const metadata: ShareMetadata = {
    contentType: meta.contentType,
    uploadedAt: new Date().toISOString(),
    originalFilename,
  };

  await env.SHARES.put(objectKey, bytes, {
    metadata,
    expirationTtl: SHARE_TTL_SECONDS,
  });

  const host = request.headers.get("host") ?? url.host;
  return jsonResponse({
    id,
    url: `https://${host}/s/${id}.${meta.ext}`,
    kind: kindParam,
  });
}

async function getShare(env: Env, idWithExt: string): Promise<Response> {
  if (!ID_WITH_EXT_RE.test(idWithExt)) {
    return new Response("invalid share id", {
      status: 400,
      headers: { ...CORS_HEADERS, "Content-Type": "text/plain; charset=utf-8" },
    });
  }

  const objectKey = `share/${idWithExt}`;
  const result = await env.SHARES.getWithMetadata<ShareMetadata>(objectKey, "arrayBuffer");
  if (!result.value) {
    return new Response("not found", {
      status: 404,
      headers: { ...CORS_HEADERS, "Content-Type": "text/plain; charset=utf-8" },
    });
  }

  const contentType = result.metadata?.contentType ?? "application/octet-stream";
  const headers = new Headers(CORS_HEADERS);
  headers.set("Content-Type", contentType);
  headers.set("Cache-Control", "public, max-age=31536000, immutable");
  headers.set("X-Content-Type-Options", "nosniff");

  return new Response(result.value, { status: 200, headers });
}

function optionsResponse(): Response {
  return new Response(null, {
    status: 204,
    headers: {
      ...CORS_HEADERS,
      "Access-Control-Allow-Methods": "POST, GET, OPTIONS",
      "Access-Control-Allow-Headers": "Authorization, Content-Type",
    },
  });
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: {
      ...CORS_HEADERS,
      "Content-Type": "application/json; charset=utf-8",
    },
  });
}

function isShareKind(value: string): value is ShareKind {
  return value === "html" || value === "png" || value === "json";
}

function randomId(length: number): string {
  const bytes = new Uint8Array(length);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (byte) => ID_ALPHABET[byte % ID_ALPHABET.length]).join("");
}
