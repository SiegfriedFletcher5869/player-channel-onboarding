use crate::infrai_client::{InfraiClient, InfraiError};
use serde::Serialize;
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignupChannel {
    Email,
    Sms,
}

impl SignupChannel {
    pub fn parse(value: &str) -> Result<Self, OnboardingError> {
        match value {
            "email" => Ok(Self::Email),
            "sms" => Ok(Self::Sms),
            other => Err(OnboardingError::InvalidChannel(other.to_owned())),
        }
    }
}

#[derive(Debug)]
pub struct PlayerSignup {
    pub email: String,
    pub phone: String,
    pub password: String,
    pub display_name: String,
    pub channel: SignupChannel,
    pub request_id: String,
}

#[derive(Debug, Serialize)]
pub struct PlayerBackendState {
    pub generated_assets: Vec<PlayerAsset>,
    pub live_events: Vec<LiveEvent>,
    pub moderation_queue: Vec<ModerationItem>,
}

#[derive(Debug, Serialize)]
pub struct PlayerAsset {
    pub kind: &'static str,
    pub state: &'static str,
}

#[derive(Debug, Serialize)]
pub struct LiveEvent {
    pub kind: &'static str,
    pub state: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ModerationItem {
    pub queue: &'static str,
    pub state: &'static str,
}

impl Default for PlayerBackendState {
    fn default() -> Self {
        Self {
            generated_assets: vec![PlayerAsset {
                kind: "avatar",
                state: "awaiting_upload",
            }],
            live_events: vec![LiveEvent {
                kind: "player_joined",
                state: "ready",
            }],
            moderation_queue: vec![ModerationItem {
                queue: "player_assets",
                state: "idle",
            }],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryChannel {
    Email,
    Sms,
}

#[derive(Debug, PartialEq, Eq)]
pub struct OnboardingReceipt {
    pub user_id: String,
    pub delivered_by: DeliveryChannel,
    pub message_id: String,
}

#[derive(Debug, Error)]
pub enum OnboardingError {
    #[error(transparent)]
    Infrai(#[from] InfraiError),
    #[error("unsupported signup channel: {0}")]
    InvalidChannel(String),
    #[error("both welcome channels are suppressed")]
    NoDeliverableChannel,
}

pub fn select_delivery(
    preferred: SignupChannel,
    email_suppressed: bool,
    sms_suppressed: bool,
) -> Result<DeliveryChannel, OnboardingError> {
    match (preferred, email_suppressed, sms_suppressed) {
        (SignupChannel::Email, false, _) => Ok(DeliveryChannel::Email),
        (SignupChannel::Sms, _, false) => Ok(DeliveryChannel::Sms),
        (SignupChannel::Email, true, false) => Ok(DeliveryChannel::Sms),
        (SignupChannel::Sms, false, true) => Ok(DeliveryChannel::Email),
        (_, true, true) => Err(OnboardingError::NoDeliverableChannel),
    }
}

pub async fn onboard_player(
    infrai: &InfraiClient,
    signup: PlayerSignup,
) -> Result<OnboardingReceipt, OnboardingError> {
    let state = PlayerBackendState::default();
    let metadata = json!({
        "signup_channel": match signup.channel {
            SignupChannel::Email => "email",
            SignupChannel::Sms => "sms",
        },
        "game_backend": state
    });
    let user = infrai
        .create_player(
            &signup.email,
            &signup.password,
            &signup.display_name,
            &metadata,
            &format!("{}:account", signup.request_id),
        )
        .await?;

    // Account data flows directly into checks and delivery on the same client.
    let (email_suppressed, sms_suppressed) = tokio::try_join!(
        infrai.email_is_suppressed(&signup.email),
        infrai.sms_is_suppressed(&signup.phone)
    )?;
    let channel = select_delivery(signup.channel, email_suppressed, sms_suppressed)?;
    let greeting = format!(
        "Welcome {}, player {} is ready.",
        signup.display_name, user.user_id
    );
    let send_key = format!("{}:welcome", signup.request_id);

    let message_id = match channel {
        DeliveryChannel::Email => {
            infrai
                .send_email(
                    &signup.email,
                    "Your player account is ready",
                    &greeting,
                    &send_key,
                )
                .await?
                .message_id
        }
        DeliveryChannel::Sms => {
            infrai
                .send_sms(&signup.phone, &greeting, &send_key)
                .await?
                .message_id
        }
    };

    Ok(OnboardingReceipt {
        user_id: user.user_id,
        delivered_by: channel,
        message_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_signup_falls_back_to_sms_when_email_is_suppressed() {
        let selected = select_delivery(SignupChannel::Email, true, false).unwrap();
        assert_eq!(selected, DeliveryChannel::Sms);
    }

    #[test]
    fn signup_stops_when_both_channels_are_suppressed() {
        let selected = select_delivery(SignupChannel::Sms, true, true);
        assert!(matches!(
            selected,
            Err(OnboardingError::NoDeliverableChannel)
        ));
    }
}
