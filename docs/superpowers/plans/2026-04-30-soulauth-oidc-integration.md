# SoulAuth OIDC Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make SoulAuth the standard OIDC identity provider for SoulBook while preserving SoulBook's own application JWT and `/sso` frontend bridge.

**Architecture:** SoulAuth provides browser login sessions, Google login, OIDC authorization code flow, token exchange, and userinfo. SoulBook becomes a confidential OIDC client that maps SoulAuth subjects to `local_user` records and signs its own app JWT. SoulBookFront only changes the login entry point and routing rewrites.

**Tech Stack:** Rust, Axum, SurrealDB, Dioxus, Vercel rewrites, Nginx reverse proxy, Google OAuth, OIDC authorization code flow with PKCE.

---

## File Map

- `/mnt/d/code/SoulAuth/src/services/oidc.rs`: complete OIDC persistence and token/user lookup logic.
- `/mnt/d/code/SoulAuth/src/routes/oidc.rs`: make browser authorize flow session-cookie aware.
- `/mnt/d/code/SoulAuth/src/routes/auth.rs`: make Google callback create browser session and resume OIDC authorization.
- `/mnt/d/code/SoulAuth/src/models/sso_session.rs`: reuse or extend existing session model if it already fits browser sessions.
- `/mnt/d/code/SoulAuth/src/config.rs`: add public issuer/session config only if current `APP_URL` cannot safely represent issuer.
- `/mnt/d/code/SoulBook/src/config.rs`: add SoulAuth OIDC client config.
- `/mnt/d/code/SoulBook/src/routes/soulauth_oidc.rs`: new SoulBook OIDC client start/callback routes.
- `/mnt/d/code/SoulBook/src/routes/mod.rs`: export new route module.
- `/mnt/d/code/SoulBook/src/main.rs`: mount new routes.
- `/mnt/d/code/SoulBook/src/routes/google_oauth.rs`: keep as fallback during rollout.
- `/mnt/d/code/SoulBookFront/src/pages/login.rs`: change main Google/SSO login link to SoulBook SoulAuth route.
- `/mnt/d/code/SoulBookFront/vercel.json`: add `/auth/:path*` rewrite.
- Server `/etc/nginx/conf.d/soulhub.conf`: add `/auth/` proxy to SoulAuth.

## Task 1: SoulAuth OIDC Persistence

**Files:**
- Modify: `/mnt/d/code/SoulAuth/src/services/oidc.rs`
- Test: `/mnt/d/code/SoulAuth/src/services/oidc.rs`

- [ ] **Step 1: Write failing tests for OIDC persistence helpers**

Add unit tests for PKCE verification. These tests avoid database setup and give a fast red/green check before editing the OIDC persistence code:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose, Engine as _};
    use sha2::{Digest, Sha256};

    #[test]
    fn verifies_s256_pkce_challenge() {
        let verifier = "abcdefghijklmnopqrstuvwxyz0123456789";
        let challenge = general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        let result = OidcService::verify_pkce_value(&challenge, Some("S256"), verifier);
        assert!(result.expect("pkce verification should succeed"));
    }

    #[test]
    fn rejects_wrong_s256_pkce_challenge() {
        let result = OidcService::verify_pkce_value("wrong", Some("S256"), "abcdefghijklmnopqrstuvwxyz0123456789");
        assert!(!result.expect("pkce verification should return false"));
    }

    #[test]
    fn verifies_plain_pkce_challenge() {
        let result = OidcService::verify_pkce_value("plain-verifier", Some("plain"), "plain-verifier");
        assert!(result.expect("plain pkce should pass"));
    }
}
```

- [ ] **Step 2: Run tests and confirm the helper does not exist**

Run:

```bash
cd /mnt/d/code/SoulAuth
cargo test services::oidc::tests::verifies_s256_pkce_challenge --lib
```

Expected: fail because `OidcService::verify_pkce_value` is not defined.

- [ ] **Step 3: Extract PKCE helper and implement database methods**

In `/mnt/d/code/SoulAuth/src/services/oidc.rs`, add a testable static helper:

```rust
pub(crate) fn verify_pkce_value(
    code_challenge: &str,
    method: Option<&str>,
    code_verifier: &str,
) -> Result<bool> {
    match method.unwrap_or("plain") {
        "S256" => {
            let hash = Sha256::digest(code_verifier.as_bytes());
            let encoded = general_purpose::URL_SAFE_NO_PAD.encode(hash);
            Ok(encoded == code_challenge)
        }
        "plain" => Ok(code_verifier == code_challenge),
        other => Err(anyhow!("Unsupported code challenge method: {}", other)),
    }
}
```

Change the instance method to call it:

```rust
fn verify_pkce(
    &self,
    code_challenge: &str,
    method: &Option<String>,
    code_verifier: &str,
) -> Result<bool> {
    Self::verify_pkce_value(code_challenge, method.as_deref(), code_verifier)
}
```

Replace all `Not implemented` OIDC methods with SurrealDB-backed implementations:

```rust
async fn get_client(&self, client_id: &str) -> Result<OidcClient> {
    let mut result = self
        .db
        .query("SELECT * FROM oidc_client WHERE client_id = $client_id AND is_active = true LIMIT 1")
        .bind(("client_id", client_id.to_string()))
        .await?;
    let clients: Vec<OidcClient> = result.take(0)?;
    clients.into_iter().next().ok_or_else(|| anyhow!("OIDC client not found"))
}

async fn get_user_by_id(&self, user_id: &str) -> Result<User> {
    let mut result = self
        .db
        .query("SELECT * FROM user WHERE id = type::record('user', $user_id) LIMIT 1")
        .bind(("user_id", user_id.to_string()))
        .await?;
    let users: Vec<User> = result.take(0)?;
    users.into_iter().next().ok_or_else(|| anyhow!("User not found"))
}

async fn save_authorization_code(&self, code: &OidcAuthorizationCode) -> Result<()> {
    self.db.query(
        "CREATE oidc_authorization_code CONTENT {
            code: $code,
            client_id: $client_id,
            user_id: $user_id,
            redirect_uri: $redirect_uri,
            scope: $scope,
            state: $state,
            nonce: $nonce,
            code_challenge: $code_challenge,
            code_challenge_method: $code_challenge_method,
            used: false,
            expires_at: $expires_at,
            created_at: $created_at
        }",
    )
    .bind(("code", code.code.clone()))
    .bind(("client_id", code.client_id.clone()))
    .bind(("user_id", code.user_id.clone()))
    .bind(("redirect_uri", code.redirect_uri.clone()))
    .bind(("scope", code.scope.clone()))
    .bind(("state", code.state.clone()))
    .bind(("nonce", code.nonce.clone()))
    .bind(("code_challenge", code.code_challenge.clone()))
    .bind(("code_challenge_method", code.code_challenge_method.clone()))
    .bind(("expires_at", code.expires_at))
    .bind(("created_at", code.created_at))
    .await?;
    Ok(())
}

async fn get_authorization_code(&self, code: &str) -> Result<OidcAuthorizationCode> {
    let mut result = self
        .db
        .query("SELECT * FROM oidc_authorization_code WHERE code = $code LIMIT 1")
        .bind(("code", code.to_string()))
        .await?;
    let codes: Vec<OidcAuthorizationCode> = result.take(0)?;
    codes.into_iter().next().ok_or_else(|| anyhow!("Authorization code not found"))
}

async fn update_authorization_code(&self, code: &OidcAuthorizationCode) -> Result<()> {
    self.db
        .query("UPDATE oidc_authorization_code SET used = $used WHERE code = $code")
        .bind(("used", code.used))
        .bind(("code", code.code.clone()))
        .await?;
    Ok(())
}

async fn save_access_token(&self, token: &OidcAccessToken) -> Result<()> {
    self.db.query(
        "CREATE oidc_access_token CONTENT {
            token: $token,
            token_type: $token_type,
            client_id: $client_id,
            user_id: $user_id,
            scope: $scope,
            expires_at: $expires_at,
            created_at: $created_at
        }",
    )
    .bind(("token", token.token.clone()))
    .bind(("token_type", token.token_type.clone()))
    .bind(("client_id", token.client_id.clone()))
    .bind(("user_id", token.user_id.clone()))
    .bind(("scope", token.scope.clone()))
    .bind(("expires_at", token.expires_at))
    .bind(("created_at", token.created_at))
    .await?;
    Ok(())
}

async fn get_access_token(&self, token: &str) -> Result<OidcAccessToken> {
    let mut result = self
        .db
        .query("SELECT * FROM oidc_access_token WHERE token = $token LIMIT 1")
        .bind(("token", token.to_string()))
        .await?;
    let tokens: Vec<OidcAccessToken> = result.take(0)?;
    tokens.into_iter().next().ok_or_else(|| anyhow!("Access token not found"))
}

async fn revoke_access_token(&self, token: &str) -> Result<()> {
    self.db
        .query("DELETE oidc_access_token WHERE token = $token")
        .bind(("token", token.to_string()))
        .await?;
    Ok(())
}
```

Disable refresh tokens for the initial SoulBook OIDC client by registering only the `authorization_code` grant. Leave `save_refresh_token`, `get_refresh_token`, and `update_refresh_token` unchanged for this phase because the SoulBook login flow does not request refresh tokens.

- [ ] **Step 4: Run tests**

Run:

```bash
cd /mnt/d/code/SoulAuth
cargo test services::oidc::tests --lib
cargo test --lib
```

Expected: PKCE tests pass. If model deserialization fails, adjust the SurrealDB model id field types in the smallest compatible way.

- [ ] **Step 5: Commit SoulAuth OIDC persistence**

```bash
cd /mnt/d/code/SoulAuth
git add src/services/oidc.rs
git commit -m "feat: persist oidc authorization flow"
```

## Task 2: SoulAuth Browser Session and Authorize Resume

**Files:**
- Modify: `/mnt/d/code/SoulAuth/src/routes/oidc.rs`
- Modify: `/mnt/d/code/SoulAuth/src/routes/auth.rs`
- Modify: `/mnt/d/code/SoulAuth/src/config.rs`
- Test: manual route verification with `curl -i` and service logs.

- [ ] **Step 1: Add session cookie constants and helpers**

In a focused module or near the route code, define:

```rust
const SOULAUTH_SESSION_COOKIE: &str = "soulauth_session";
const OIDC_RETURN_COOKIE: &str = "soulauth_oidc_return";
const SESSION_TTL_SECONDS: i64 = 86400;
```

Add helpers to build secure cookies:

```rust
fn build_cookie(name: &str, value: &str, max_age_seconds: i64) -> String {
    format!(
        "{}={}; Path=/; Max-Age={}; HttpOnly; Secure; SameSite=Lax",
        name,
        urlencoding::encode(value),
        max_age_seconds
    )
}
```

- [ ] **Step 2: Change authorize to use browser session**

In `/mnt/d/code/SoulAuth/src/routes/oidc.rs`, keep Bearer support for API callers, but add cookie support before redirecting to login:

```rust
let user_id_from_cookie = headers
    .get(header::COOKIE)
    .and_then(|value| value.to_str().ok())
    .and_then(|cookies| extract_cookie(cookies, SOULAUTH_SESSION_COOKIE))
    .and_then(|session| validate_session_cookie(&session, &config.jwt_secret).ok());
```

When no user is authenticated, store the original authorize query as `OIDC_RETURN_COOKIE` and redirect to:

```text
/api/auth/login/google
```

The response must include `Set-Cookie` for the return URL.

- [ ] **Step 3: Make Google callback create SoulAuth browser session**

In `/mnt/d/code/SoulAuth/src/routes/auth.rs`, after `handle_google_callback` succeeds, create a signed session token containing the SoulAuth user id and expiry. Set it in `SOULAUTH_SESSION_COOKIE`.

If `OIDC_RETURN_COOKIE` exists, redirect back to that authorize URL and clear the return cookie. Otherwise keep the existing redirect to `${APP_URL}/oauth/callback?token=...` for compatibility.

- [ ] **Step 4: Verify manually with curl headers**

Run SoulAuth locally or on server, then verify:

```bash
curl -i "http://127.0.0.1:8080/api/oidc/authorize?response_type=code&client_id=missing&redirect_uri=http%3A%2F%2Flocalhost%2Fcallback&scope=openid"
```

Expected: redirect to Google login or login route, not a JSON 500. The response should set an OIDC return cookie.

- [ ] **Step 5: Commit SoulAuth browser session flow**

```bash
cd /mnt/d/code/SoulAuth
git add src/routes/oidc.rs src/routes/auth.rs src/config.rs
git commit -m "feat: support browser oidc login sessions"
```

## Task 3: SoulBook OIDC Client Routes

**Files:**
- Modify: `/mnt/d/code/SoulBook/src/config.rs`
- Create: `/mnt/d/code/SoulBook/src/routes/soulauth_oidc.rs`
- Modify: `/mnt/d/code/SoulBook/src/routes/mod.rs`
- Modify: `/mnt/d/code/SoulBook/src/main.rs`
- Test: `/mnt/d/code/SoulBook/src/routes/soulauth_oidc.rs`

- [ ] **Step 1: Add config tests**

Add tests proving SoulAuth config is optional and parsed when present:

```rust
#[test]
fn soulauth_config_is_absent_without_env() {
    std::env::remove_var("SOULAUTH_ISSUER");
    std::env::remove_var("SOULAUTH_CLIENT_ID");
    std::env::remove_var("SOULAUTH_CLIENT_SECRET");
    std::env::remove_var("SOULAUTH_REDIRECT_URI");
    let cfg = Config::from_env_for_test();
    assert!(cfg.oauth.soulauth.is_none());
}
```

If `Config::from_env_for_test` does not exist, first extract environment parsing into a helper that can be tested without requiring production secrets.

- [ ] **Step 2: Add SoulAuth config struct**

In `/mnt/d/code/SoulBook/src/config.rs`, extend `OAuthConfig`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OAuthConfig {
    pub google: Option<GoogleOAuthConfig>,
    pub soulauth: Option<SoulAuthOidcConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoulAuthOidcConfig {
    pub issuer: String,
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub post_logout_redirect_uri: Option<String>,
}
```

Parse:

```rust
soulauth: match (
    env::var("SOULAUTH_ISSUER"),
    env::var("SOULAUTH_CLIENT_ID"),
    env::var("SOULAUTH_CLIENT_SECRET"),
    env::var("SOULAUTH_REDIRECT_URI"),
) {
    (Ok(issuer), Ok(client_id), Ok(client_secret), Ok(redirect_uri))
        if !issuer.is_empty() && !client_id.is_empty() && !client_secret.is_empty() && !redirect_uri.is_empty() =>
    {
        Some(SoulAuthOidcConfig {
            issuer,
            client_id,
            client_secret,
            redirect_uri,
            post_logout_redirect_uri: env::var("SOULAUTH_POST_LOGOUT_REDIRECT_URI").ok(),
        })
    }
    _ => None,
}
```

- [ ] **Step 3: Create route module with start and callback**

Create `/mnt/d/code/SoulBook/src/routes/soulauth_oidc.rs` with:

```rust
pub fn router() -> Router {
    Router::new()
        .route("/start", get(start))
        .route("/callback", get(callback))
}
```

The start route must:

- Read `app_state.config.oauth.soulauth`.
- Generate `state`, `nonce`, and PKCE verifier/challenge.
- Encode a short-lived state JWT containing `next`, `nonce`, and `code_verifier`.
- Redirect to `{issuer}/api/oidc/authorize`.

The callback route must:

- Reject `error` callback params with a login redirect.
- Validate the state JWT.
- POST form data to `{issuer}/api/oidc/token`.
- Call `{issuer}/api/oidc/userinfo`.
- Require `email` and `email_verified == true`.
- Find or create `local_user` by `provider = "soulauth"` and `external_subject = sub`.
- Link by verified email only if no external subject exists.
- Sign the existing SoulBook `Claims`.
- Redirect through `/sso?token=...&next=...`.

- [ ] **Step 4: Add focused tests for state and URL construction**

Test helpers should verify:

```rust
assert!(authorize_url.contains("response_type=code"));
assert!(authorize_url.contains("scope=openid"));
assert!(authorize_url.contains("code_challenge_method=S256"));
assert!(authorize_url.contains("state="));
```

- [ ] **Step 5: Mount route**

In `/mnt/d/code/SoulBook/src/routes/mod.rs`:

```rust
pub mod soulauth_oidc;
```

In `/mnt/d/code/SoulBook/src/main.rs`, mount before broad auth routes:

```rust
.nest("/api/docs/auth/soulauth", routes::soulauth_oidc::router())
```

- [ ] **Step 6: Run SoulBook tests**

```bash
cd /mnt/d/code/SoulBook
cargo test routes::soulauth_oidc --lib
cargo test --lib
```

Expected: route helper tests pass and existing tests continue passing.

- [ ] **Step 7: Commit SoulBook OIDC client**

```bash
cd /mnt/d/code/SoulBook
git add src/config.rs src/routes/soulauth_oidc.rs src/routes/mod.rs src/main.rs
git commit -m "feat: add soulauth oidc login"
```

## Task 4: Frontend Login Entry and Vercel Rewrite

**Files:**
- Modify: `/mnt/d/code/SoulBookFront/src/pages/login.rs`
- Modify: `/mnt/d/code/SoulBookFront/vercel.json`

- [ ] **Step 1: Change login link**

Change the main Google login link:

```rust
href: "/api/docs/auth/soulauth/start",
```

Keep the visible label as Google login or change to:

```rust
span { "使用 Google / SoulAuth 登录" }
```

- [ ] **Step 2: Add Vercel auth rewrite**

In `/mnt/d/code/SoulBookFront/vercel.json`, add before catch-all rewrites:

```json
{
  "source": "/auth/:path*",
  "destination": "http://47.236.185.219/auth/:path*"
}
```

- [ ] **Step 3: Build frontend**

Run the repository's Dioxus production build:

```bash
cd /mnt/d/code/SoulBookFront
dx build --release
```

Expected: build succeeds and generated assets load locally or in Vercel preview.

- [ ] **Step 4: Commit frontend changes**

```bash
cd /mnt/d/code/SoulBookFront
git add src/pages/login.rs vercel.json
git commit -m "feat: route login through soulauth"
```

## Task 5: Server Routing and Environment

**Files:**
- Server: `/etc/nginx/conf.d/soulhub.conf`
- Server: `/root/soulhub/SoulAuth/.env`
- Server: `/root/soulhub/SoulBook/.env`

- [ ] **Step 1: Back up server config**

```bash
ssh -i /home/kiki/.ssh/id_ed25519_soulbook root@47.236.185.219 \
  'cp /etc/nginx/conf.d/soulhub.conf /etc/nginx/conf.d/soulhub.conf.bak.$(date +%Y%m%d%H%M%S)'
```

- [ ] **Step 2: Add Nginx `/auth/` proxy**

Add:

```nginx
location /auth/ {
    proxy_pass http://127.0.0.1:8080/;
    proxy_http_version 1.1;
    proxy_set_header Host $host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;
}
```

- [ ] **Step 3: Validate and reload Nginx**

```bash
ssh -i /home/kiki/.ssh/id_ed25519_soulbook root@47.236.185.219 \
  'nginx -t && systemctl reload nginx'
```

Expected: `syntax is ok` and `test is successful`.

- [ ] **Step 4: Register the SoulBook OIDC client**

Generate the client secret:

```bash
openssl rand -base64 32
```

Store the generated value as `SOULBOOK_OIDC_CLIENT_SECRET` in the shell for the rest of this task:

```bash
export SOULBOOK_OIDC_CLIENT_SECRET="paste-the-openssl-output-here"
```

Create or update the SoulBook client in SoulAuth's database with client id `soulbook-web`, redirect URI `https://soul-book-front.vercel.app/api/docs/auth/soulauth/callback`, scopes `openid profile email`, response type `code`, grant type `authorization_code`, and `require_pkce = true`. Store `sha256(SOULBOOK_OIDC_CLIENT_SECRET)` in `client_secret_hash` to match SoulAuth's current verifier.

- [ ] **Step 5: Set SoulAuth environment**

Set:

```text
APP_URL=https://soul-book-front.vercel.app/auth
OAUTH_REDIRECT_URL=https://soul-book-front.vercel.app/auth/api/auth/callback
```

Keep existing Google client id and secret only in SoulAuth.

- [ ] **Step 6: Set SoulBook environment**

Set:

```text
SOULAUTH_ISSUER=https://soul-book-front.vercel.app/auth
SOULAUTH_CLIENT_ID=soulbook-web
SOULAUTH_CLIENT_SECRET=<same value generated by openssl for SOULBOOK_OIDC_CLIENT_SECRET>
SOULAUTH_REDIRECT_URI=https://soul-book-front.vercel.app/api/docs/auth/soulauth/callback
SOULAUTH_POST_LOGOUT_REDIRECT_URI=https://soul-book-front.vercel.app/docs/login
```

- [ ] **Step 7: Verify public discovery**

```bash
curl -i https://soul-book-front.vercel.app/auth/.well-known/openid-configuration
```

Expected: HTTP 200 and JSON issuer `https://soul-book-front.vercel.app/auth`.

## Task 6: Build, Deploy, and Verify

**Files:**
- Server repos: `/root/soulhub/SoulAuth`, `/root/soulhub/SoulBook`, `/root/soulhub/SoulBookFront`

- [ ] **Step 1: Push all local commits**

```bash
git -C /mnt/d/code/SoulAuth push hunter main
git -C /mnt/d/code/SoulBook push hunter main
git -C /mnt/d/code/SoulBookFront push hunter main
```

- [ ] **Step 2: Deploy SoulAuth first**

On server:

```bash
cd /root/soulhub/SoulAuth
git fetch origin main
git checkout main
git pull --ff-only origin main
rm -rf target
CC=clang CXX=clang++ cargo build --release
systemctl restart rainbow-auth
systemctl status rainbow-auth --no-pager | head -n 20
```

Expected: service active.

- [ ] **Step 3: Deploy SoulBook second**

Build on server to avoid local `libssl.so.3` and GLIBC mismatch:

```bash
cd /root/soulhub/SoulBook
git fetch origin main
git checkout main
git pull --ff-only origin main
rm -rf target
cargo build --release
systemctl restart soulbook
systemctl status soulbook --no-pager | head -n 20
```

Expected: service active.

- [ ] **Step 4: Verify or trigger Vercel deployment**

Confirm latest GitHub commit is deployed in Vercel. If Vercel does not auto-deploy, run:

```bash
cd /mnt/d/code/SoulBookFront
vercel --prod
```

Expected: production deployment finishes and reports `https://soul-book-front.vercel.app`.

- [ ] **Step 5: End-to-end browser verification**

Verify:

```text
https://soul-book-front.vercel.app/docs/login
```

Expected:

- Login button goes to SoulBook `/api/docs/auth/soulauth/start`.
- Browser reaches SoulAuth authorize flow.
- Google login returns to SoulAuth callback.
- SoulAuth returns code to SoulBook callback.
- SoulBook redirects through `/sso`.
- Browser lands on `/docs/` authenticated.
- Refresh keeps user logged in.
- Document tree API returns non-500.

- [ ] **Step 6: Server log verification**

```bash
ssh -i /home/kiki/.ssh/id_ed25519_soulbook root@47.236.185.219 \
  'journalctl -u rainbow-auth -u soulbook --since "20 minutes ago" --no-pager | tail -n 200'
```

Expected: no panic, no `Not implemented`, no token exchange failure, no userinfo failure.

## Rollback

If SoulAuth login fails after deployment:

1. Revert SoulBookFront login link to `/api/docs/auth/google/start`.
2. Keep SoulBook direct Google route enabled.
3. Reload Vercel deployment.
4. Restore previous SoulAuth/SoulBook binaries from server backups if services fail to start.
5. Restore Nginx config backup if `/auth/` proxy causes routing issues.

## Plan Self-Review

- Spec coverage: SoulAuth OIDC persistence, browser session, SoulBook OIDC client, frontend login switch, Vercel/Nginx routing, deployment, verification, and rollback are covered.
- Placeholder scan: no unresolved implementation placeholders remain. Secret values are intentionally generated at deployment time and are not committed.
- Type consistency: planned route names and env names match the design spec. SoulBook uses its own JWT after SoulAuth userinfo, preserving existing `/sso` frontend behavior.
