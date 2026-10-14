//! AI Market Protocol v2 consumer SDK for Rust.
//!
//! Target: Tauri desktop apps, native CLI tools.
//!
//! # Example
//! ```no_run
//! use aimarket_agent::{AimarketAgent, AimarketAgentConfig, ChannelDeposit};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Credits rail: the account behind the key pays; no channel is opened.
//!     let agent = AimarketAgent::with_config(AimarketAgentConfig {
//!         api_key: Some(std::env::var("AIMARKET_API_KEY")?),
//!         ..AimarketAgentConfig::new("https://modelmarket.dev", "your-wallet-private-key-hex")
//!     });
//!
//!     let plan = agent.discover("ATS scoring for fintech roles", Some(1.0), Some(5), Some("career")).await?;
//!     let cap = &plan[0].capability;
//!     let result = agent
//!         .invoke(&cap.capability_id, serde_json::json!({"role": "PM"}), "",
//!                 Some(&cap.product_id), Some(&cap.source_hub), None)
//!         .await?;
//!     println!("Spent: ${}", result.price_usd);
//!
//!     // Channel rail: an agent without a key pays from a channel funded by the on-chain
//!     // deposit you already made (production hubs verify it and require the payer proof).
//!     let channel_agent = AimarketAgent::new("https://modelmarket.dev", "your-wallet-private-key-hex");
//!     let deposit = ChannelDeposit {
//!         tx_hash: "0x<deposit transaction>".into(),
//!         payer_signature: Some("0x<signature from payer_proof::sign_channel_open_proof>".into()),
//!         ..Default::default()
//!     };
//!     let channel = channel_agent.open_channel(5.0, "USDC", "base", Some(&deposit)).await?;
//!     channel_agent
//!         .invoke(&cap.capability_id, serde_json::json!({"role": "PM"}), &channel.channel_id,
//!                 Some(&cap.product_id), Some(&cap.source_hub), None)
//!         .await?;
//!     channel_agent.close_channel(&channel.channel_id).await?;
//!     Ok(())
//! }
//! ```

pub mod agent;
pub mod eip712;
pub mod error;
pub mod models;
pub mod payer_proof;
pub mod signer;
pub mod tee;

pub use agent::{AimarketAgent, AimarketAgentConfig, ChannelDeposit};
pub use error::AimarketError;
pub use models::*;
pub use eip712::{
    compute_debit_digest as eip712_compute_debit_digest, DebitDigestParams,
    DEBIT_TYPEHASH_HEADER, ESCROW_CONTRACT_NAME, ESCROW_CONTRACT_VERSION,
};
pub use signer::{compute_debit_digest, DebitAuthorization, MarketSigner};
pub use tee::{TeeVerificationResult, TeeVerifier, TrustedHashCache};
