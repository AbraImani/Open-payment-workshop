use std::env;
use std::error::Error;
use std::io::{self, Write};
use std::path::PathBuf;

use open_payments::client::{AuthenticatedClient, AuthenticatedResources, ClientConfig};
use open_payments::types::{
    AccessItem, AccessTokenRequest, Amount, CreateIncomingPaymentRequest,
    CreateOutgoingPaymentRequest, GrantRequest, GrantResponse, IncomingPaymentAction,
    InteractFinish, InteractRequest, LimitsOutgoing, OutgoingPaymentAction,
};
use serde_json::json;
use url::Url;

type AppResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[tokio::main]
async fn main() -> AppResult<()> {
    dotenv::dotenv().ok();

    let client_wallet_address_url = env_var("CLIENT_WALLET_ADDRESS_URL")?;
    let sending_wallet_address_url = env_var("SENDING_WALLET_ADDRESS_URL")?;
    let receiving_wallet_address_url = env_var("RECEIVING_WALLET_ADDRESS_URL")?;
    let key_id = env_var("KEY_ID")?;
    let private_key_path = PathBuf::from(env_var("PRIVATE_KEY_PATH")?);
    let interact_finish_uri = env::var("INTERACT_FINISH_URI")
        .unwrap_or_else(|_| "http://localhost/callback".to_string());

    let client = AuthenticatedClient::new(ClientConfig {
        private_key_path,
        key_id,
        jwks_path: None,
        wallet_address_url: client_wallet_address_url.clone(),
    })?;

    println!("Initialized client for {}", client_wallet_address_url);
    pause()?;

    let sending_wallet_address = client
        .wallet_address()
        .get(&sending_wallet_address_url)
        .await?;
    let receiving_wallet_address = client
        .wallet_address()
        .get(&receiving_wallet_address_url)
        .await?;

    println!("Got wallet addresses");
    println!("Sending wallet: {sending_wallet_address:#?}");
    println!("Receiving wallet: {receiving_wallet_address:#?}");
    pause()?;

    let total_amount: i64 = 5000;

    let incoming_grant_request = GrantRequest::new(
        AccessTokenRequest {
            access: vec![AccessItem::IncomingPayment {
                actions: vec![IncomingPaymentAction::Create],
                identifier: None,
            }],
        },
        None,
    );

    let incoming_grant = client
        .grant()
        .request(&receiving_wallet_address.auth_server, &incoming_grant_request)
        .await?;

    let incoming_access_token = match incoming_grant {
        GrantResponse::WithToken { access_token, .. } => access_token.value,
        GrantResponse::WithInteraction { .. } => {
            return Err("Expected a finalized incoming payment grant".into())
        }
    };

    let incoming_payment = client
        .incoming_payments()
        .create(
            &receiving_wallet_address.resource_server,
            &CreateIncomingPaymentRequest {
                wallet_address: receiving_wallet_address.id.clone(),
                incoming_amount: Some(Amount {
                    value: total_amount.to_string(),
                    asset_code: receiving_wallet_address.asset_code.clone(),
                    asset_scale: receiving_wallet_address.asset_scale,
                }),
                expires_at: None,
                metadata: Some(json!({"description": "Book order"})),
            },
            Some(&incoming_access_token),
        )
        .await?;

    println!("Created incoming payment: {incoming_payment:#?}");
    pause()?;

    let outgoing_grant_request = GrantRequest::new(
        AccessTokenRequest {
            access: vec![AccessItem::OutgoingPayment {
                actions: vec![
                    OutgoingPaymentAction::Create,
                    OutgoingPaymentAction::Read,
                    OutgoingPaymentAction::ReadAll,
                    OutgoingPaymentAction::List,
                ],
                identifier: sending_wallet_address.id.clone(),
                limits: Some(LimitsOutgoing {
                    debit_amount: Some(Amount {
                        value: total_amount.to_string(),
                        asset_code: sending_wallet_address.asset_code.clone(),
                        asset_scale: sending_wallet_address.asset_scale,
                    }),
                    receive_amount: None,
                    interval: None,
                }),
            }],
        },
        Some(InteractRequest {
            start: vec!["redirect".to_string()],
            finish: Some(InteractFinish {
                method: "redirect".to_string(),
                uri: interact_finish_uri.clone(),
                nonce: "open-payments-workshop".to_string(),
            }),
        }),
    );

    let outgoing_grant = client
        .grant()
        .request(&sending_wallet_address.auth_server, &outgoing_grant_request)
        .await?;

    let outgoing_access_token = match outgoing_grant {
        GrantResponse::WithInteraction {
            interact,
            continue_,
        } => {
            println!("Open this URL in your browser and accept the grant:");
            println!("{}", interact.redirect);
            println!("When the browser returns to the callback URL, paste the full URL here.");
            println!("Continue token: {}", continue_.access_token.value);
            println!("Continue URI: {}", continue_.uri);
            pause()?;
            let interact_ref_input = prompt(
                "Paste the callback URL or interact_ref for the outgoing grant continuation: ",
            )?;
            let interact_ref = extract_interact_ref(&interact_ref_input)?;

            let finalized_outgoing_grant = client
                .grant()
                .continue_grant(&continue_.uri, &interact_ref, Some(&continue_.access_token.value))
                .await?;

            match finalized_outgoing_grant {
                open_payments::types::ContinueResponse::WithToken { access_token, .. } => {
                    access_token.value
                }
                open_payments::types::ContinueResponse::Pending { .. } => {
                    return Err("Expected finalized outgoing payment grant".into())
                }
            }
        }
        GrantResponse::WithToken { access_token, .. } => {
            println!("Outgoing grant finished without interaction.");
            access_token.value
        }
    };

    println!("Finalized outgoing grant");
    pause()?;

    let half_amount = (total_amount / 2).to_string();

    let first_outgoing_payment = client
        .outgoing_payments()
        .create(
            &sending_wallet_address.resource_server,
            &CreateOutgoingPaymentRequest::FromIncomingPayment {
                wallet_address: sending_wallet_address.id.clone(),
                incoming_payment_id: incoming_payment.id.clone(),
                debit_amount: Amount {
                    value: half_amount.clone(),
                    asset_code: sending_wallet_address.asset_code.clone(),
                    asset_scale: sending_wallet_address.asset_scale,
                },
                metadata: Some(json!({"description": "First payment (for book one)"})),
            },
            Some(&outgoing_access_token),
        )
        .await?;

    println!("Created outgoing payment: {first_outgoing_payment:#?}");
    pause()?;

    let second_outgoing_payment = client
        .outgoing_payments()
        .create(
            &sending_wallet_address.resource_server,
            &CreateOutgoingPaymentRequest::FromIncomingPayment {
                wallet_address: sending_wallet_address.id.clone(),
                incoming_payment_id: incoming_payment.id.clone(),
                debit_amount: Amount {
                    value: half_amount,
                    asset_code: sending_wallet_address.asset_code.clone(),
                    asset_scale: sending_wallet_address.asset_scale,
                },
                metadata: Some(json!({"description": "Second payment (for book two)"})),
            },
            Some(&outgoing_access_token),
        )
        .await?;

    println!("Created second outgoing payment: {second_outgoing_payment:#?}");
    Ok(())
}

fn env_var(name: &str) -> AppResult<String> {
    env::var(name)
        .map_err(|_| format!("Environment variable {name} is not set").into())
}

fn prompt(message: &str) -> AppResult<String> {
    print!("{message}");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

fn pause() -> AppResult<()> {
    let _ = prompt("Press Enter to continue... ")?;
    Ok(())
}

fn extract_interact_ref(input: &str) -> AppResult<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Empty interact_ref input".into());
    }

    if let Ok(url) = Url::parse(trimmed) {
        if let Some((_, value)) = url.query_pairs().find(|(key, _)| key == "interact_ref") {
            return Ok(value.to_string());
        }
        return Err("The callback URL does not contain an interact_ref query parameter".into());
    }

    Ok(trimmed.to_string())
}
