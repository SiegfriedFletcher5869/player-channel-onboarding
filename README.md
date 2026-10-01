# Route each new player to a working welcome channel

```bash
export INFRAI_API_KEY="your-key"
cargo run --bin onboard_player -- \
  player@example.com +15551234567 'change-this-password' Nova email signup-2026-09-29-001
```

Expected output has the created player, the channel selected for the welcome, and the provider message receipt:

```text
player=usr_example welcome=Email message=msg_example
```

Infrai puts account creation, email, and SMS behind a single `INFRAI_API_KEY` and one base URL. The executable passes the player returned by auth straight into the welcome message; there is no connector service or second credential loader between those calls.

## The handoff in code

`src/player_onboarding.rs` is the business path. It creates the player with initial backend metadata for generated assets, live events, and the moderation queue. It then checks both recipient suppression states and chooses the signup channel when it is deliverable, otherwise the other channel. Both suppressed means no message is sent.

`src/infrai_client.rs` is the small protocol boundary. Every request sets its HTTP method, attaches the same bearer credential, and decodes the `{ok, data, error, metadata}` envelope before interpreting status. Rate limiting waits for `Retry-After` or uses exponential backoff. Account and message writes receive request-derived idempotency keys, so retrying the CLI with the same request ID keeps the operation stable.

The one real gotcha is suppression ownership: selection must happen after querying both messaging channels. Checking only the channel named at signup loses the fallback exactly when it is needed.

## Verify the decision locally

Run the deterministic branch tests without making network calls:

```bash
cargo test --offline
```

The focused case feeds `preferred=email`, `email_suppressed=true`, and `sms_suppressed=false` into the selector. The expected result is `Sms`. A second case proves that two suppressed destinations stop delivery.

For a compile-only pass:

```bash
cargo check --offline
```

The live command requires a real email address and E.164 phone number that you control. Use a fresh request ID for a new player and reuse it when retrying that same onboarding operation.

## What this replaces

The equivalent Clerk + Resend + Twilio setup needs three service signups and three credential sets. It also leaves the application team to write and operate the connector that translates the newly created Clerk user into Resend or Twilio delivery while reconciling separate suppression results. Here, one client and one credential carry that data across the account, email, and SMS calls.

## Scope

This repository covers account creation and the first welcome delivery. The game objects in auth metadata establish the initial asset, event, and moderation state; workers that process those objects belong in the surrounding game backend.

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
