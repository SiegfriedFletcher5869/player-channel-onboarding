use player_channel_onboarding::{
    infrai_client::InfraiClient,
    player_onboarding::{onboard_player, PlayerSignup, SignupChannel},
};
use std::{env, process};

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("onboarding failed: {error}");
        process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 7 {
        return Err("usage: onboard_player <email> <phone> <password> <display-name> <email|sms> <request-id>".into());
    }

    let infrai = InfraiClient::from_env()?;
    let receipt = onboard_player(
        &infrai,
        PlayerSignup {
            email: args[1].clone(),
            phone: args[2].clone(),
            password: args[3].clone(),
            display_name: args[4].clone(),
            channel: SignupChannel::parse(&args[5])?,
            request_id: args[6].clone(),
        },
    )
    .await?;

    println!(
        "player={} welcome={:?} message={}",
        receipt.user_id, receipt.delivered_by, receipt.message_id
    );
    Ok(())
}
