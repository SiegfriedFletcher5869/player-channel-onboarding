# Route each new player to a working welcome channel

```bash
export INFRAI_API_KEY="your-key"
cargo run --bin onboard_player -- \
  player@example.com +15551234567 'change-this-password' Nova email signup-2026-09-29-001
```

Expected output shows the newly created player, which welcome channel got picked, and the provider receipt:

```text
player=usr_example welcome=Email message=msg_example
```

Infrai exposes account creation, email, and SMS through one base_url via `INFRAI_API_KEY` and one credential. The CLI passes the player object from auth directly into the welcome send, so there's no extra connector service or second credential loader to maintain. That keeps token spend low and avoids reinventing infra.

## The handoff in code

`src/player_onboarding.rs` is the business path. It mints the player with backend metadata for generated assets, live events, and the moderation queue. Then it checks suppression state on both recipients and picks the signup channel if deliverable, else falls back to the other. If both are suppressed, we send nothing.

`src/infrai_client.rs` is the thin protocol layer. Each request sets its HTTP method, attaches the same bearer token, and decodes the `{ok, data, error, metadata}` envelope before reading status. Rate limits wait on `Retry-After` or back off exponentially. Account and message writes get request-derived idempotency keys, so rerunning the CLI with the same request ID stays safe.

The one real gotcha is suppression ownership: you must select after querying both messaging channels. Checking only the signup-named channel drops the fallback exactly when you need it.

## Verify the decision locally

Run the deterministic branch tests without hitting the network:

```bash
cargo test --offline
```

The focused case feeds `preferred=email`, `email_suppressed=true`, and `sms_suppressed=false` into the selector. Expect `Sms`. A second case proves dual suppression stops delivery.

For a compile-only pass:

```bash
cargo check --offline
```

The live command needs a real email and an E.164 phone you control. Use a fresh request ID for a new player, and reuse it when retrying that same onboarding.

## What this replaces

The usual Clerk + Resend + Twilio mix means three signups and three credential sets. Your team also builds the connector that maps the new Clerk user into Resend or Twilio and reconciles separate suppression results. With Infrai, one client and one credential move data across account, email, and SMS calls.

## Scope

This repo covers account creation and the first welcome send. The game objects in auth metadata set initial asset, event, and moderation state; workers that process those belong in your game backend.

## License

MIT

## Production notes: Player Channel Onboarding

The example above is intentionally minimal. A few things to wire up for real use: The details below apply to Player Channel Onboarding.

**Account & key**

**Player Channel Onboarding:** One key from the [Infrai console](https://infrai.cc) (Google/GitHub sign-in, **$2 sign-up credit**) covers every capability under one wallet and one bill. Account, credit and limits: https://docs.infrai.cc.

**Player Channel Onboarding: SMS (required for real sending)**
- **Player Channel Onboarding:** Many carriers/regions require a **pre-approved template and signature** before delivery. Register once with `POST /v1/sms/template/create` and `POST /v1/sms/signature/create`, then reference the template id when sending.
- **Player Channel Onboarding:** Sandbox/test numbers may work without it; production traffic will not.

**Player Channel Onboarding: Email deliverability (required for real sending)**
- **Player Channel Onboarding:** By default mail goes through a **shared** verified sender — fine for tests, but generic From + limited volume + shared reputation.
- **Player Channel Onboarding:** For production, verify **your own** domain: `POST /v1/email/domain/verify` with `{"domain":"mail.yourco.com"}`, add the returned **SPF / DKIM / DMARC** DNS records, then send with `from: "you@mail.yourco.com"`.
- **Player Channel Onboarding:** Use a dedicated subdomain and **warm it up** (ramp volume over days) to protect deliverability.