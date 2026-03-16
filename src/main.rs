use std::fs;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
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
    /// Authenticate with GitHub
    Login,
    /// Request on-chain bitcoin from the faucet
    Onchain {
        /// Bitcoin address or BIP21 URI
        address: String,
        /// Amount in satoshis
        #[arg(default_value = "10000")]
        sats: u64,
    },
    /// Pay a lightning invoice, LNURL, or zap a nostr pubkey
    Lightning {
        /// Bolt11 invoice, LNURL, lightning address, or npub
        bolt11: String,
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

fn token_path() -> PathBuf {
    let dir = home_dir().join(".mutinynet");
    dir.join("token")
}

fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn load_token() -> Option<String> {
    fs::read_to_string(token_path())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn save_token(token: &str) -> Result<()> {
    let path = token_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, token)?;
    Ok(())
}

fn get_token(cli: &Cli) -> Result<String> {
    cli.token.clone().or_else(load_token).context(
        "No token found. Run `mutinynet-cli login` or set --token / MUTINYNET_FAUCET_TOKEN",
    )
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

fn post_json(url: &str, body: &Value, token: Option<&str>) -> Result<Value> {
    let json_body = serde_json::to_string(body)?;
    let mut req = bitreq::post(url)
        .with_header("Content-Type", "application/json")
        .with_body(json_body.into_bytes());
    if let Some(token) = token {
        req = req.with_header("Authorization", format!("Bearer {token}"));
    }
    let resp = req.send().context("Failed to send request")?;
    if resp.status_code >= 200 && resp.status_code < 300 {
        let text = resp.as_str()?;
        Ok(serde_json::from_str(text)?)
    } else {
        let text = resp.as_str().unwrap_or("unknown error");
        bail!("{}: {}", resp.status_code, text)
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

fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Command::Login => login(&cli.url)?,
        Command::Onchain { address, sats } => {
            let token = get_token(&cli)?;
            let body = post_json(
                &format!("{}/api/onchain", cli.url),
                &json!({ "address": address, "sats": *sats }),
                Some(&token),
            )?;
            println!("{}", body["txid"].as_str().unwrap_or(&body.to_string()));
        }
        Command::Lightning { bolt11 } => {
            let token = get_token(&cli)?;
            let body = post_json(
                &format!("{}/api/lightning", cli.url),
                &json!({ "bolt11": bolt11 }),
                Some(&token),
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
            let token = get_token(&cli)?;
            let body = post_json(
                &format!("{}/api/channel", cli.url),
                &json!({
                    "pubkey": pubkey,
                    "capacity": *capacity,
                    "push_amount": *push_amount,
                    "host": host,
                }),
                Some(&token),
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
