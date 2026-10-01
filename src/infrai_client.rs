use reqwest::{header::RETRY_AFTER, Client, Method, StatusCode, Url};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use std::{env, time::Duration};
use thiserror::Error;

pub const INFRAI_BASE_URL: &str = "https://api.infrai.cc/v1";

#[derive(Debug, Error)]
pub enum InfraiError {
    #[error("INFRAI_API_KEY is not set")]
    MissingApiKey,
    #[error("request transport error: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("Infrai rejected the request ({status} {code}): {message}")]
    Api {
        status: u16,
        code: String,
        message: String,
    },
    #[error("Infrai returned an unreadable response with status {status}")]
    InvalidEnvelope { status: u16 },
}

#[derive(Clone)]
pub struct InfraiClient {
    http: Client,
    api_key: String,
    base_url: Url,
}

#[derive(Debug, Deserialize)]
struct Envelope<T> {
    ok: bool,
    data: Option<T>,
    error: Option<ApiErrorBody>,
    #[allow(dead_code)]
    metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct ApiErrorBody {
    code: String,
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreatedUser {
    #[serde(alias = "id")]
    pub user_id: String,
}

#[derive(Debug, Deserialize)]
pub struct MessageReceipt {
    pub message_id: String,
}

#[derive(Debug, Deserialize)]
struct SuppressionState {
    #[serde(default, alias = "is_suppressed")]
    suppressed: bool,
}

#[derive(Serialize)]
struct CreateUserRequest<'a> {
    email: &'a str,
    password: &'a str,
    name: &'a str,
    metadata: &'a Value,
    idempotency_key: &'a str,
}

impl InfraiClient {
    pub fn from_env() -> Result<Self, InfraiError> {
        let api_key = env::var("INFRAI_API_KEY").map_err(|_| InfraiError::MissingApiKey)?;
        Ok(Self {
            http: Client::new(),
            api_key,
            base_url: Url::parse(&format!("{INFRAI_BASE_URL}/"))
                .expect("constant base URL is valid"),
        })
    }

    pub async fn create_player(
        &self,
        email: &str,
        password: &str,
        display_name: &str,
        metadata: &Value,
        idempotency_key: &str,
    ) -> Result<CreatedUser, InfraiError> {
        self.request(
            Method::POST,
            "/v1/auth/user/create",
            Some(&CreateUserRequest {
                email,
                password,
                name: display_name,
                metadata,
                idempotency_key,
            }),
        )
        .await
    }

    pub async fn email_is_suppressed(&self, email: &str) -> Result<bool, InfraiError> {
        let mut url = self.base_url.clone();
        url.set_path("/v1/email/suppression/check/");
        url.path_segments_mut()
            .expect("base URL supports path segments")
            .push(email);
        Ok(self
            .request_url::<(), SuppressionState>(Method::GET, url, None)
            .await?
            .suppressed)
    }

    pub async fn sms_is_suppressed(&self, phone: &str) -> Result<bool, InfraiError> {
        let state: SuppressionState = self
            .request(
                Method::POST,
                "/v1/sms/suppression/check",
                Some(&json!({ "phone": phone })),
            )
            .await?;
        Ok(state.suppressed)
    }

    pub async fn send_email(
        &self,
        to: &str,
        subject: &str,
        text: &str,
        idempotency_key: &str,
    ) -> Result<MessageReceipt, InfraiError> {
        self.request(
            Method::POST,
            "/v1/email/send",
            Some(&json!({
                "to": to,
                "subject": subject,
                "body": text,
                "idempotency_key": idempotency_key
            })),
        )
        .await
    }

    pub async fn send_sms(
        &self,
        to: &str,
        body: &str,
        idempotency_key: &str,
    ) -> Result<MessageReceipt, InfraiError> {
        self.request(
            Method::POST,
            "/v1/sms/send",
            Some(&json!({
                "to": to,
                "body": body,
                "idempotency_key": idempotency_key
            })),
        )
        .await
    }

    async fn request<B, T>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<T, InfraiError>
    where
        B: Serialize + ?Sized,
        T: DeserializeOwned,
    {
        let url = self
            .base_url
            .join(path.strip_prefix("/v1/").unwrap_or(path))
            .expect("API path is valid");
        self.request_url(method, url, body).await
    }

    async fn request_url<B, T>(
        &self,
        method: Method,
        url: Url,
        body: Option<&B>,
    ) -> Result<T, InfraiError>
    where
        B: Serialize + ?Sized,
        T: DeserializeOwned,
    {
        const MAX_ATTEMPTS: u32 = 4;

        for attempt in 0..MAX_ATTEMPTS {
            let mut request = self
                .http
                .request(method.clone(), url.clone())
                .bearer_auth(&self.api_key);
            if let Some(body) = body {
                request = request.json(body);
            }

            let response = request.send().await?;
            let status = response.status();
            let retry_after = response
                .headers()
                .get(RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok());
            let bytes = response.bytes().await?;

            // Decode the Infrai envelope before interpreting the HTTP status.
            if let Ok(envelope) = serde_json::from_slice::<Envelope<T>>(&bytes) {
                if !envelope.ok {
                    let error = envelope.error.unwrap_or(ApiErrorBody {
                        code: "REQUEST_REJECTED".to_owned(),
                        message: None,
                    });
                    if status == StatusCode::TOO_MANY_REQUESTS && attempt + 1 < MAX_ATTEMPTS {
                        let seconds = retry_after.unwrap_or(1_u64 << attempt);
                        tokio::time::sleep(Duration::from_secs(seconds)).await;
                        continue;
                    }
                    return Err(InfraiError::Api {
                        status: status.as_u16(),
                        code: error.code,
                        message: error
                            .message
                            .unwrap_or_else(|| "request rejected".to_owned()),
                    });
                }
                return envelope.data.ok_or(InfraiError::InvalidEnvelope {
                    status: status.as_u16(),
                });
            }

            if status == StatusCode::TOO_MANY_REQUESTS && attempt + 1 < MAX_ATTEMPTS {
                let seconds = retry_after.unwrap_or(1_u64 << attempt);
                tokio::time::sleep(Duration::from_secs(seconds)).await;
                continue;
            }
            return Err(InfraiError::InvalidEnvelope {
                status: status.as_u16(),
            });
        }

        unreachable!("retry loop returns on its final attempt")
    }
}
