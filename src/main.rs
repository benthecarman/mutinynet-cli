use std::fs;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use lightning_invoice::Bolt11Invoice;
use serde_json::{json, Value};

const DEFAULT_URL: &str = "https://faucet.mutinynet.com";

#[derive(Parser)]
#[command(name = "mutinynet-cli", about = "CLI for the Mutinynet faucet")]
struct Cli {
    /// Faucet URL
    #[arg(long, default_value = DEFAULT_URL, env = "MUTINYNET_FAUCET_URL")]
    url: String,

    /// Auth token (overrides stored token)
    #[arg(long, env = "MUTINYNET_FAUCET_TOKEN")]
    token: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Authenticate with GitHub or Lightning (L402)
    Login {
        /// Use Lightning payment (L402) instead of GitHub
        #[arg(long)]
        lightning: bool,
    },
    /// Request on-chain bitcoin from the faucet
    Onchain {
        /// Bitcoin address or BIP21 URI
        address: String,
        /// Amount in satoshis
        #[arg(default_value = "10000")]
        sats: u64,
    },
    /// Pay or decode a lightning invoice, LNURL, or zap a nostr pubkey
    Lightning {
        /// Bolt11 invoice to pay or decode, or an LNURL, lightning address, or npub to pay
        bolt11: String,
        /// Decode a bolt11 invoice instead of paying it
        #[arg(short = 'd', long = "decode")]
        decode: bool,
    },
    /// Open a lightning channel from the faucet node
    Channel {
        /// Pubkey of your node
        pubkey: String,
        /// Channel capacity in satoshis
        capacity: u64,
        /// Amount to push to your side in satoshis
        #[arg(long, default_value = "0")]
        push_amount: u64,
        /// Your node's address (host:port)
        #[arg(long)]
        host: Option<String>,
    },
    /// Generate a bolt11 invoice from the faucet node
    Bolt11 {
        /// Amount in satoshis (omit for zero-amount)
        amount: Option<u64>,
    },
}

fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn data_dir() -> PathBuf {
    home_dir().join(".mutinynet")
}

fn token_path() -> PathBuf {
    data_dir().join("token")
}

fn l402_path() -> PathBuf {
    data_dir().join("l402")
}

fn load_token() -> Option<String> {
    fs::read_to_string(token_path())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn load_l402() -> Option<(String, String)> {
    let content = fs::read_to_string(l402_path()).ok()?;
    let content = content.trim();
    let (token, preimage) = content.split_once(':')?;
    if token.is_empty() || preimage.is_empty() {
        return None;
    }
    Some((token.to_string(), preimage.to_string()))
}

fn save_file(path: &PathBuf, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)?;
    Ok(())
}

fn save_token(token: &str) -> Result<()> {
    save_file(&token_path(), token)
}

fn save_l402(token: &str, preimage: &str) -> Result<()> {
    save_file(&l402_path(), &format!("{token}:{preimage}"))
}

/// Where the auth credentials came from.
enum AuthSource {
    /// Passed via --token flag or env var (don't delete anything).
    Flag,
    /// Loaded from ~/.mutinynet/l402
    L402,
    /// Loaded from ~/.mutinynet/token
    Token,
}

/// Build the Authorization header value from stored credentials.
/// Prefers CLI --token, then L402 credentials, then stored JWT.
/// Returns the header value and the source so we can clean up on 401.
fn get_auth_header(cli: &Cli) -> Result<(String, AuthSource)> {
    if let Some(token) = &cli.token {
        return Ok((format!("Bearer {token}"), AuthSource::Flag));
    }
    if let Some((token, preimage)) = load_l402() {
        return Ok((format!("L402 {token}:{preimage}"), AuthSource::L402));
    }
    if let Some(token) = load_token() {
        return Ok((format!("Bearer {token}"), AuthSource::Token));
    }
    bail!("No token found. Run `mutinynet-cli login` or `mutinynet-cli login --lightning` or set --token / MUTINYNET_FAUCET_TOKEN")
}

/// Delete stored credentials for the given auth source and return an
/// informative error telling the user to re-authenticate.
fn clear_expired_credentials(source: AuthSource) -> Result<Value> {
    match source {
        AuthSource::L402 => {
            let _ = fs::remove_file(l402_path());
            bail!("Authentication expired. Removed stored L402 credentials.\nRun `mutinynet-cli login --lightning` to re-authenticate.")
        }
        AuthSource::Token => {
            let _ = fs::remove_file(token_path());
            bail!("Authentication expired. Removed stored token.\nRun `mutinynet-cli login` to re-authenticate.")
        }
        AuthSource::Flag => {
            bail!("Authentication failed. The provided token is invalid or expired.")
        }
    }
}

fn get_json(url: &str) -> Result<Value> {
    let resp = bitreq::get(url).send().context("Failed to send request")?;
    if resp.status_code >= 200 && resp.status_code < 300 {
        let text = resp.as_str()?;
        Ok(serde_json::from_str(text)?)
    } else {
        let text = resp.as_str().unwrap_or("unknown error");
        bail!("{}: {}", resp.status_code, text)
    }
}

struct ApiResponse {
    status_code: u16,
    body: Value,
}

fn post_json_raw(url: &str, body: &Value, auth: Option<&str>) -> Result<ApiResponse> {
    let json_body = serde_json::to_string(body)?;
    let mut req = bitreq::post(url)
        .with_header("Content-Type", "application/json")
        .with_body(json_body.into_bytes());
    if let Some(auth) = auth {
        req = req.with_header("Authorization", auth);
    }
    let resp = req.send().context("Failed to send request")?;
    let status_code = resp.status_code as u16;
    let text = resp.as_str().unwrap_or("unknown error");
    let body = serde_json::from_str(text).unwrap_or_else(|_| json!({"error": text}));
    Ok(ApiResponse { status_code, body })
}

fn post_json(url: &str, body: &Value, auth: Option<&str>) -> Result<Value> {
    let resp = post_json_raw(url, body, auth)?;
    if resp.status_code >= 200 && resp.status_code < 300 {
        Ok(resp.body)
    } else {
        bail!("{}: {}", resp.status_code, resp.body)
    }
}

/// Make an authenticated POST request. On 401, clear stored credentials and
/// tell the user to re-authenticate.
fn authed_post(url: &str, body: &Value, cli: &Cli) -> Result<Value> {
    let (auth, source) = get_auth_header(cli)?;
    let resp = post_json_raw(url, body, Some(&auth))?;
    if resp.status_code == 401 {
        clear_expired_credentials(source)
    } else if resp.status_code >= 200 && resp.status_code < 300 {
        Ok(resp.body)
    } else {
        bail!("{}: {}", resp.status_code, resp.body)
    }
}

fn post_form(url: &str, body: &str) -> Result<Value> {
    let resp = bitreq::post(url)
        .with_header("Content-Type", "application/x-www-form-urlencoded")
        .with_header("Accept", "application/json")
        .with_body(body.as_bytes().to_vec())
        .send()
        .context("Failed to send request")?;
    if resp.status_code >= 200 && resp.status_code < 300 {
        let text = resp.as_str()?;
        Ok(serde_json::from_str(text)?)
    } else {
        let text = resp.as_str().unwrap_or("unknown error");
        bail!("{}: {}", resp.status_code, text)
    }
}

fn login_lightning(faucet_url: &str) -> Result<()> {
    // Request L402 challenge from the faucet
    let resp = post_json(&format!("{faucet_url}/api/l402"), &json!({}), None)?;

    let invoice = resp["invoice"]
        .as_str()
        .context("Missing invoice in L402 response")?;
    let token = resp["token"]
        .as_str()
        .context("Missing token in L402 response")?;

    println!("Pay this Lightning invoice to authenticate:");
    println!();
    println!("{invoice}");
    println!();
    println!("Waiting for payment...");

    // Poll for payment
    let preimage = loop {
        std::thread::sleep(std::time::Duration::from_secs(2));

        let check_url = format!("{faucet_url}/api/l402/check?token={token}");
        let check_resp = get_json(&check_url)?;

        match check_resp["status"].as_str() {
            Some("settled") => {
                let preimage = check_resp["preimage"]
                    .as_str()
                    .context("Missing preimage in settled response")?
                    .to_string();
                break preimage;
            }
            Some("expired") => bail!("Invoice expired. Try again."),
            Some("pending") => continue,
            other => bail!("Unexpected status: {:?}", other),
        }
    };

    save_l402(token, &preimage)?;
    println!(
        "Authenticated via Lightning! Credentials saved to {}",
        l402_path().display()
    );
    Ok(())
}

fn login(faucet_url: &str) -> Result<()> {
    // Fetch the GitHub client ID from the faucet
    let resp = get_json(&format!("{faucet_url}/auth/github/client_id"))?;
    let client_id = resp["client_id"]
        .as_str()
        .context("Failed to get client_id from faucet")?;

    // Start GitHub device flow
    let body = format!("client_id={client_id}&scope=user:email");
    let device_resp = post_form("https://github.com/login/device/code", &body)?;

    let device_code = device_resp["device_code"]
        .as_str()
        .context("Missing device_code")?;
    let user_code = device_resp["user_code"]
        .as_str()
        .context("Missing user_code")?;
    let verification_uri = device_resp["verification_uri"]
        .as_str()
        .context("Missing verification_uri")?;
    let interval = device_resp["interval"].as_u64().unwrap_or(5);

    println!("Go to: {verification_uri}");
    println!("Enter code: {user_code}");
    println!();
    println!("Waiting for authorization...");

    // Poll for the access token
    let access_token = loop {
        std::thread::sleep(std::time::Duration::from_secs(interval));

        let poll_body = format!(
            "client_id={client_id}&device_code={device_code}&grant_type=urn:ietf:params:oauth:grant-type:device_code"
        );
        let poll_resp = post_form("https://github.com/login/oauth/access_token", &poll_body)?;

        if let Some(token) = poll_resp["access_token"].as_str() {
            break token.to_string();
        }

        match poll_resp["error"].as_str() {
            Some("authorization_pending") => continue,
            Some("slow_down") => {
                std::thread::sleep(std::time::Duration::from_secs(5));
                continue;
            }
            Some(err) => bail!("GitHub auth error: {err}"),
            None => bail!("Unexpected response: {poll_resp}"),
        }
    };

    // Exchange GitHub access token for faucet JWT
    let faucet_resp = post_json(
        &format!("{faucet_url}/auth/github/device"),
        &json!({ "code": access_token }),
        None,
    )?;

    let jwt = faucet_resp["token"]
        .as_str()
        .context("Missing token in faucet response")?;

    save_token(jwt)?;
    println!("Logged in! Token saved to {}", token_path().display());
    Ok(())
}

fn decode_invoice(bolt11: &str) -> Result<Value> {
    let invoice = Bolt11Invoice::from_str(bolt11)
        .map_err(|err| anyhow::anyhow!("Failed to decode bolt11 invoice: {err}"))?;
    let expires_at_unix = invoice.expires_at().map(|expiry| expiry.as_secs());
    let fallback_addresses: Vec<String> = invoice
        .fallback_addresses()
        .into_iter()
        .map(|address| address.to_string())
        .collect();

    Ok(json!({
        "invoice": bolt11,
        "network": format!("{:?}", invoice.currency()),
        "amount_msat": invoice.amount_milli_satoshis(),
        "description": invoice.description().to_string(),
        "payment_hash": invoice.payment_hash().to_string(),
        "payee_pubkey": invoice.get_payee_pub_key().to_string(),
        "created_at_unix": invoice.duration_since_epoch().as_secs(),
        "expiry_secs": invoice.expiry_time().as_secs(),
        "expires_at_unix": expires_at_unix,
        "is_expired": invoice.is_expired(),
        "min_final_cltv_expiry_delta": invoice.min_final_cltv_expiry_delta(),
        "route_hint_count": invoice.route_hints().len(),
        "fallback_addresses": fallback_addresses,
    }))
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Command::Login { lightning } => {
            if *lightning {
                login_lightning(&cli.url)?;
            } else {
                login(&cli.url)?;
            }
        }
        Command::Onchain { address, sats } => {
            let body = authed_post(
                &format!("{}/api/onchain", cli.url),
                &json!({ "address": address, "sats": *sats }),
                &cli,
            )?;
            println!("{}", body["txid"].as_str().unwrap_or(&body.to_string()));
        }
        Command::Lightning { bolt11, decode } => {
            if *decode {
                let body = decode_invoice(bolt11)?;
                println!("{}", serde_json::to_string_pretty(&body)?);
                return Ok(());
            }
            let body = authed_post(
                &format!("{}/api/lightning", cli.url),
                &json!({ "bolt11": bolt11 }),
                &cli,
            )?;
            println!(
                "{}",
                body["payment_hash"].as_str().unwrap_or(&body.to_string())
            );
        }
        Command::Channel {
            pubkey,
            capacity,
            push_amount,
            host,
        } => {
            let body = authed_post(
                &format!("{}/api/channel", cli.url),
                &json!({
                    "pubkey": pubkey,
                    "capacity": *capacity,
                    "push_amount": *push_amount,
                    "host": host,
                }),
                &cli,
            )?;
            println!("{}", body["txid"].as_str().unwrap_or(&body.to_string()));
        }
        Command::Bolt11 { amount } => {
            let body = post_json(
                &format!("{}/api/bolt11", cli.url),
                &json!({ "amount_sats": amount }),
                None,
            )?;
            println!("{}", body["bolt11"].as_str().unwrap_or(&body.to_string()));
        }
    }

    Ok(())
}
