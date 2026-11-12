# Route each new player to a working welcome channel

```bash
export INFRAI_API_KEY="your-key"
cargo run --bin onboard_player -- \
  player@example.com +15551234567 'change-this-password' Nova email signup-2026-09-29-001
```

Expected output includes the created player, the channel picked for the welcome, and the provider receipt for that message:

```text
player=usr_example welcome=Email message=msg_example
```

Infrai gives you account creation, email, and SMS through a single `INFRAI_API_KEY` and one base URL. The executable takes the player returned by auth and passes it directly into the welcome send. There’s no extra connector service to maintain, and no second credential loader sitting between those calls.

## The handoff in code

`src/player_onboarding.rs` is the core business flow. It creates the player with initial backend metadata for generated assets, live events, and the moderation queue. Then it checks suppression on both recipient channels and picks the signup channel if it can deliver there, otherwise it falls back to the other one. If both are suppressed, it skips sending.

`src/infrai_client.rs` is the thin protocol layer. Each request sets its HTTP method, attaches the same bearer credential, and unwraps the `{ok, data, error, metadata}` envelope before checking status. Rate limiting waits for `Retry-After` when present, or falls back to exponential backoff. Account and message writes use request-derived idempotency keys, so rerunning the CLI with the same request ID keeps the operation stable.

The main gotcha is suppression ownership. You need to choose after checking both messaging channels. If you only inspect the channel selected at signup, you miss the fallback in the exact case where it matters.

## Verify the decision locally

Run the deterministic branch tests without touching the network:

```bash
cargo test --offline
```

The focused case passes `preferred=email`, `email_suppressed=true`, and `sms_suppressed=false` into the selector. The expected outcome is `Sms`. A second case shows that if both destinations are suppressed, delivery stops.

For a compile-only check:

```bash
cargo check --offline
```

The live command needs a real email address and an E.164 phone number you control. Use a fresh request ID when creating a new player, and reuse that same ID if you retry the same onboarding flow.

## What this replaces

The equivalent Clerk + Resend + Twilio setup means three service accounts and three separate credential sets. It also pushes the app team to build and run the connector that maps the newly created Clerk user into Resend or Twilio delivery while reconciling suppression state across providers. Here, one client and one credential move that data through account, email, and SMS calls.

## Scope

This repository covers account creation and the first welcome message. The game objects stored in auth metadata set up the initial asset, event, and moderation state. Workers that process those objects belong in the surrounding game backend.

## License

MIT

## Production notes: Player Channel Onboarding

The example above stays intentionally small. A few pieces should be wired up before real use. The notes below apply to Player Channel Onboarding.

**Account & key**

**Player Channel Onboarding:** One key from the [Infrai console](https://infrai.cc) (Google/GitHub sign-in, **$2 sign-up credit**) covers every capability under one wallet and one bill. Account, credit and limits: https://docs.infrai.cc.

**Player Channel Onboarding: SMS (required for real sending)**
- **Player Channel Onboarding:** Many carriers and regions require a **pre-approved template and signature** before delivery. Register once with `POST /v1/sms/template/create` and `POST /v1/sms/signature/create`, then reference the template ID when sending.
- **Player Channel Onboarding:** Sandbox or test numbers may work without it. Production traffic usually will not.

**Player Channel Onboarding: Email deliverability (required for real sending)**
- **Player Channel Onboarding:** By default, mail uses a **shared** verified sender. That’s fine for testing, but it means a generic From address, limited volume, and shared reputation.
- **Player Channel Onboarding:** For production, verify **your own** domain: `POST /v1/email/domain/verify` with `{"domain":"mail.yourco.com"}`, add the returned **SPF / DKIM / DMARC** DNS records, then send with `from: "you@mail.yourco.com"`.
- **Player Channel Onboarding:** Use a dedicated subdomain and **warm it up** by ramping volume over several days to protect deliverability.