# BetterThanYou Cloudflare Sharing

This Worker receives raw report assets from the BetterThanYou CLI, stores them in Cloudflare Workers KV (free-tier friendly, no card-on-file required), and serves stable public URLs such as `https://better-than-you.nomadamas.org/s/<id>.html`. The CLI tries this endpoint first when configured, then falls back to its existing free-host providers.

The Worker lives on a dedicated subdomain (`better-than-you.nomadamas.org` by default), so paths are bare (`/share`, `/s/<id>.<ext>`, `/healthz`). Earlier revisions used `/btyu/*` paths on the apex to dodge Cloudflare's default WAF rules; moving to a subdomain side-steps that entirely and is also cleaner from a sharing perspective.

## Prerequisites

- Node 20+
- Cloudflare account
- `nomadamas.org` zone on Cloudflare, or any other Cloudflare-managed zone you want to substitute

## Deploy

1. Install dependencies:

   ```bash
   cd infra/cloudflare && npm install
   ```

2. Log in to Cloudflare:

   ```bash
   npx wrangler login
   ```

3. Create the KV namespace:

   ```bash
   npx wrangler kv namespace create btyu-shares
   ```

   Copy the `id` from the output and paste it into `wrangler.toml` under the `[[kv_namespaces]]` block (replace the placeholder).

4. Create the publish token secret:

   ```bash
   openssl rand -hex 32 | tee /tmp/btyu-token   # save this; the CLI needs it
   cat /tmp/btyu-token | npx wrangler secret put PUBLISH_TOKEN
   ```

5. Edit `wrangler.toml`:
   - Set `account_id` to the Cloudflare account that owns the zone.
   - The `routes` block is already enabled with `better-than-you.nomadamas.org` as a Worker custom domain. Adjust `pattern` and `zone_name` if you use a different subdomain or zone.

6. Deploy:

   ```bash
   npx wrangler deploy
   ```

7. Test:

   ```bash
   curl https://better-than-you.nomadamas.org/healthz
   ```

   Expected response: `ok`.

## Set The CLI Endpoint

Configure these values in your shell or through the BetterThanYou Settings menu entries for public sharing:

```bash
export BTYU_PUBLISH_URL=https://better-than-you.nomadamas.org
export BTYU_PUBLISH_TOKEN=<the PUBLISH_TOKEN secret>
```

`better-than-you publish --copy` will then upload to this Worker first and return a `better-than-you.nomadamas.org/s/<id>.html` URL. If the Worker upload fails, the CLI continues to its existing fallback hosts.

## Lifecycle / Cleanup

In the Cloudflare dashboard, optionally add an R2 lifecycle rule to expire `share/` objects after N days.

## Cost

Cloudflare free-tier limits comfortably cover personal use: Workers 100k requests/day, R2 10 GB storage plus 1M writes and 10M reads/month, and R2 egress is free.
