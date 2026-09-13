use kaspa_addresses::{Address, Prefix, Version};
use secp256k1::{Keypair, Secp256k1, SecretKey};
use std::env;
use std::str::FromStr;

pub struct Config {
    pub network: String,
    pub rpc_url: String,
    private_key: Option<SecretKey>,
}

impl Config {
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        dotenvy::dotenv().ok();
        let network = env::var("KASPA_NETWORK").unwrap_or_else(|_| "testnet-10".into());
        if network != "testnet-10" {
            return Err(
                format!("unsupported network: {network}; only testnet-10 is accepted").into(),
            );
        }
        let rpc_url = env::var("KASPA_RPC_URL")?;
        let private_key = match env::var("KASPA_PRIVATE_KEY").unwrap_or_default().trim() {
            "" => None,
            value => Some(
                SecretKey::from_str(value)
                    .map_err(|_| "KASPA_PRIVATE_KEY must be a 32-byte hexadecimal key")?,
            ),
        };
        Ok(Self {
            network,
            rpc_url,
            private_key,
        })
    }

    pub fn address(&self) -> Result<Address, Box<dyn std::error::Error>> {
        let secret = self
            .private_key
            .as_ref()
            .ok_or("KASPA_PRIVATE_KEY is empty in .env; add a testnet-10 key first")?;
        let keypair = Keypair::from_secret_key(&Secp256k1::new(), secret);
        let public_key = keypair.x_only_public_key().0.serialize();
        Ok(Address::new(Prefix::Testnet, Version::PubKey, &public_key))
    }
}
