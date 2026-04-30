use axum::{
    extract::{Extension, Query},
    http::{
        header::{COOKIE, SET_COOKIE},
        HeaderMap,
    },
    response::{IntoResponse, Redirect, Response},
    routing::get,
    Router,
};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    error::{AppError, Result},
    services::auth::Claims,
    AppState,
};

pub fn router() -> Router {
    Router::new()
        .route("/start", get(start))
        .route("/callback", get(callback))
}

const STATE_COOKIE_NAME: &str = "soulbook_google_oauth_csrf";
const STATE_COOKIE_PATH: &str = "/api/docs/auth/google";

#[derive(Debug, Deserialize)]
struct StartParams {
    next: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OAuthState {
    nonce: String,
    next: Option<String>,
    binding: String,
    exp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GoogleLoginCookie {
    binding: String,
    exp: i64,
}

#[derive(Debug, Deserialize)]
struct CallbackParams {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

async fn start(
    Extension(app_state): Extension<Arc<AppState>>,
    Query(params): Query<StartParams>,
) -> Result<Response> {
    let oauth = app_state
        .config
        .oauth
        .google
        .as_ref()
        .ok_or_else(|| AppError::BadRequest("Google login is not configured".into()))?;

    let binding = Uuid::new_v4().simple().to_string();
    let state_claims = OAuthState {
        nonce: Uuid::new_v4().to_string(),
        next: Some(crate::sanitize_sso_next(
            params.next,
            &app_state.config.server.app_url,
        )),
        binding: binding.clone(),
        exp: (Utc::now() + Duration::minutes(10)).timestamp(),
    };
    let login_cookie = GoogleLoginCookie {
        binding,
        exp: (Utc::now() + Duration::minutes(10)).timestamp(),
    };
    let state = encode_state(&state_claims, &app_state.config.auth.jwt_secret)?;
    let cookie_token = encode_login_cookie(&login_cookie, &app_state.config.auth.jwt_secret)?;

    let url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth\
         ?response_type=code\
         &client_id={}\
         &redirect_uri={}\
         &scope={}\
         &state={}\
         &access_type=online\
         &prompt=select_account",
        urlencoding::encode(&oauth.client_id),
        urlencoding::encode(&oauth.redirect_uri),
        urlencoding::encode("openid email profile"),
        urlencoding::encode(&state),
    );

    let mut response = Redirect::to(&url).into_response();
    response.headers_mut().append(
        SET_COOKIE,
        state_cookie(&cookie_token)
            .parse()
            .map_err(|e| AppError::Internal(anyhow::anyhow!("state cookie failed: {}", e)))?,
    );

    Ok(response)
}

async fn callback(
    Extension(app_state): Extension<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<CallbackParams>,
) -> Result<Response> {
    if params.error.is_some() {
        let state_token = params
            .state
            .ok_or_else(|| AppError::BadRequest("missing state".into()))?;
        let state_claims = decode_state(&state_token, &app_state.config.auth.jwt_secret)?;
        let login_cookie = login_cookie_from_headers(&headers, &app_state.config.auth.jwt_secret)?;
        validate_login_cookie(login_cookie, &state_claims)?;
        return redirect_with_clear_cookie("/docs/login?error=login_failed");
    }
    let code = params
        .code
        .ok_or_else(|| AppError::BadRequest("missing code".into()))?;
    let state_token = params
        .state
        .ok_or_else(|| AppError::BadRequest("missing state".into()))?;

    let state_claims = decode_state(&state_token, &app_state.config.auth.jwt_secret)?;
    let login_cookie = login_cookie_from_headers(&headers, &app_state.config.auth.jwt_secret)?;
    validate_login_cookie(login_cookie, &state_claims)?;

    let oauth = app_state
        .config
        .oauth
        .google
        .as_ref()
        .ok_or_else(|| AppError::BadRequest("Google login is not configured".into()))?;

    let http = reqwest::Client::new();

    // Step 1: exchange authorization code for access_token
    let token_resp = http
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("code", code.as_str()),
            ("client_id", oauth.client_id.as_str()),
            ("client_secret", oauth.client_secret.as_str()),
            ("redirect_uri", oauth.redirect_uri.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Google token request failed: {}", e)))?;

    if !token_resp.status().is_success() {
        let body = token_resp.text().await.unwrap_or_default();
        return Err(AppError::BadRequest(format!(
            "Google token exchange failed: {}",
            body
        )));
    }

    let token_json: Value = token_resp
        .json()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Google token parse failed: {}", e)))?;

    let access_token = token_json
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("Google did not return access_token")))?;

    // Step 2: fetch user profile
    let userinfo: Value = http
        .get("https://www.googleapis.com/oauth2/v3/userinfo")
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Google userinfo request failed: {}", e)))?
        .json()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Google userinfo parse failed: {}", e)))?;

    let email = userinfo
        .get("email")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::BadRequest("Google account has no email".into()))?
        .trim()
        .to_lowercase();
    let name = userinfo
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let picture = userinfo
        .get("picture")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Step 3: find or create local_user by email
    let db = &app_state.db.client;
    let mut select_result = db
        .query("SELECT type::string(id) AS id FROM local_user WHERE email = $email LIMIT 1")
        .bind(("email", &email))
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("local_user lookup failed: {}", e)))?;
    let existing: Vec<Value> = select_result
        .take(0)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("local_user parse failed: {}", e)))?;

    let user_id = if let Some(row) = existing.into_iter().next() {
        let raw_id = row
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .replace("local_user:", "");
        raw_id
            .trim_matches(|c: char| c == '`' || c == '⟨' || c == '⟩' || c == '"' || c == ' ')
            .to_string()
    } else {
        let new_id = Uuid::new_v4().to_string();
        db.query(
            "CREATE local_user SET
                id = type::record('local_user', $user_id),
                email = $email,
                username = $username,
                password_hash = '',
                avatar_url = $avatar,
                provider = 'google',
                created_at = time::now()",
        )
        .bind(("user_id", &new_id))
        .bind(("email", &email))
        .bind(("username", name.unwrap_or_default()))
        .bind(("avatar", picture.unwrap_or_default()))
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("local_user create failed: {}", e)))?;
        new_id
    };

    // Step 4: issue JWT
    let claims = Claims {
        sub: user_id,
        exp: (Utc::now() + Duration::seconds(app_state.config.auth.jwt_expiration as i64))
            .timestamp(),
        iat: Utc::now().timestamp(),
        session_id: None,
    };
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(app_state.config.auth.jwt_secret.as_ref()),
    )
    .map_err(|e| AppError::Internal(anyhow::anyhow!("jwt encode failed: {}", e)))?;

    // Step 5: redirect to /sso bridge — it will inject token into localStorage
    let next = crate::sanitize_sso_next(state_claims.next, &app_state.config.server.app_url);
    let bridge_binding = crate::create_sso_bridge_binding();
    let bridge = crate::create_sso_bridge_handle(
        &token,
        Some(next.clone()),
        &app_state.config.server.app_url,
        &bridge_binding,
    )?;
    let sso_url = format!(
        "/sso?bridge={}&next={}",
        urlencoding::encode(&bridge),
        urlencoding::encode(&next),
    );

    let mut response = redirect_with_clear_cookie(&sso_url)?;
    response.headers_mut().append(
        SET_COOKIE,
        crate::sso_bridge_binding_cookie(&bridge_binding)
            .parse()
            .map_err(|e| AppError::Internal(anyhow::anyhow!("bridge cookie failed: {}", e)))?,
    );

    Ok(response)
}

fn encode_state(state: &OAuthState, secret: &str) -> Result<String> {
    encode(
        &Header::default(),
        state,
        &EncodingKey::from_secret(secret.as_ref()),
    )
    .map_err(|e| AppError::Internal(anyhow::anyhow!("state encode failed: {}", e)))
}

fn decode_state(token: &str, secret: &str) -> Result<OAuthState> {
    let validation = Validation::new(Algorithm::HS256);
    decode::<OAuthState>(
        token,
        &DecodingKey::from_secret(secret.as_ref()),
        &validation,
    )
    .map(|data| data.claims)
    .map_err(|_| AppError::BadRequest("invalid or expired state".into()))
}

fn encode_login_cookie(cookie: &GoogleLoginCookie, secret: &str) -> Result<String> {
    encode(
        &Header::default(),
        cookie,
        &EncodingKey::from_secret(secret.as_ref()),
    )
    .map_err(|e| AppError::Internal(anyhow::anyhow!("login cookie encode failed: {}", e)))
}

fn decode_login_cookie(token: &str, secret: &str) -> Result<GoogleLoginCookie> {
    let validation = Validation::new(Algorithm::HS256);
    decode::<GoogleLoginCookie>(
        token,
        &DecodingKey::from_secret(secret.as_ref()),
        &validation,
    )
    .map(|data| data.claims)
    .map_err(|_| AppError::BadRequest("invalid or expired state".into()))
}

fn validate_login_cookie(
    cookie: Option<GoogleLoginCookie>,
    state: &OAuthState,
) -> Result<GoogleLoginCookie> {
    match cookie {
        Some(cookie)
            if cookie.binding == state.binding
                && !cookie.binding.trim().is_empty()
                && cookie.exp > Utc::now().timestamp() =>
        {
            Ok(cookie)
        }
        _ => Err(AppError::BadRequest("invalid or expired state".into())),
    }
}

fn state_cookie(cookie_token: &str) -> String {
    format!(
        "{}={}; Max-Age=600; Path={}; HttpOnly; Secure; SameSite=Lax",
        STATE_COOKIE_NAME, cookie_token, STATE_COOKIE_PATH
    )
}

fn clear_state_cookie() -> String {
    format!(
        "{}=; Max-Age=0; Path={}; HttpOnly; Secure; SameSite=Lax",
        STATE_COOKIE_NAME, STATE_COOKIE_PATH
    )
}

fn redirect_with_clear_cookie(url: &str) -> Result<Response> {
    let mut response = Redirect::to(url).into_response();
    response.headers_mut().append(
        SET_COOKIE,
        clear_state_cookie()
            .parse()
            .map_err(|e| AppError::Internal(anyhow::anyhow!("clear state cookie failed: {}", e)))?,
    );
    Ok(response)
}

fn login_cookie_from_headers(
    headers: &HeaderMap,
    secret: &str,
) -> Result<Option<GoogleLoginCookie>> {
    let Some(cookie_header) = headers.get(COOKIE) else {
        return Ok(None);
    };
    let Ok(cookie_header) = cookie_header.to_str() else {
        return Ok(None);
    };
    let cookie_token = cookie_header.split(';').find_map(|cookie| {
        let (name, value) = cookie.trim().split_once('=')?;
        (name == STATE_COOKIE_NAME).then(|| value.to_string())
    });

    cookie_token
        .map(|token| decode_login_cookie(&token, secret))
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{header::COOKIE, HeaderMap};

    #[test]
    fn google_state_cookie_must_match_state_binding() {
        let state = OAuthState {
            nonce: "nonce-value".to_string(),
            next: Some("/docs".to_string()),
            binding: "binding-value".to_string(),
            exp: (Utc::now() + Duration::minutes(10)).timestamp(),
        };
        let cookie = GoogleLoginCookie {
            binding: "binding-value".to_string(),
            exp: (Utc::now() + Duration::minutes(10)).timestamp(),
        };

        assert!(validate_login_cookie(Some(cookie), &state).is_ok());
        assert!(validate_login_cookie(None, &state).is_err());
        assert!(validate_login_cookie(
            Some(GoogleLoginCookie {
                binding: "other-value".to_string(),
                exp: (Utc::now() + Duration::minutes(10)).timestamp(),
            }),
            &state
        )
        .is_err());
    }

    #[test]
    fn google_login_cookie_round_trips_from_cookie_header() {
        let cookie = GoogleLoginCookie {
            binding: "binding-value".to_string(),
            exp: (Utc::now() + Duration::minutes(10)).timestamp(),
        };
        let token = encode_login_cookie(&cookie, "test-secret").expect("cookie should encode");
        let mut headers = HeaderMap::new();
        headers.insert(
            COOKIE,
            format!("other=1; {}={}", STATE_COOKIE_NAME, token)
                .parse()
                .expect("cookie header should parse"),
        );

        let decoded = login_cookie_from_headers(&headers, "test-secret")
            .expect("cookie should decode")
            .expect("cookie should exist");

        assert_eq!(decoded.binding, "binding-value");
    }
}
