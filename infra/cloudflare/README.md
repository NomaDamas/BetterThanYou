# BetterThanYou Cloudflare Sharing

This Worker receives raw report assets from the BetterThanYou CLI, stores them in Cloudflare Workers KV (free-tier friendly, no card-on-file required), and serves stable public URLs such as `https://nomadamas.org/btyu/s/<id>.html`. The CLI tries this endpoint first when configured, then falls back to its existing free-host providers.

All routes live under the `/btyu/*` prefix to avoid Cloudflare's default WAF rules that block POSTs to common paths like `/upload`.

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
   - The `routes` block is already enabled with `nomadamas.org/btyu/*`. Adjust `pattern` and `zone_name` if you use a different domain.

6. Deploy:

   ```bash
   npx wrangler deploy
   ```

7. Test:

   ```bash
   curl https://nomadamas.org/btyu/healthz
   ```

   Expected response: `ok`.

## Set The CLI Endpoint

Configure these values in your shell or through the BetterThanYou Settings menu entries for public sharing:

```bash
export BTYU_PUBLISH_URL=https://nomadamas.org
export BTYU_PUBLISH_TOKEN=<the PUBLISH_TOKEN secret>
```

`better-than-you publish --copy` will then upload to this Worker first and return a `nomadamas.org/btyu/s/<id>.html` URL. If the Worker upload fails, the CLI continues to its existing fallback hosts.

## Lifecycle / Cleanup

In the Cloudflare dashboard, optionally add an R2 lifecycle rule to expire `share/` objects after N days.

## Cost

Cloudflare free-tier limits comfortably cover personal use: Workers 100k requests/day, R2 10 GB storage plus 1M writes and 10M reads/month, and R2 egress is free.
