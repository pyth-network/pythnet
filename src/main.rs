//! This application implements an RPC and P2P gossip layer for PythNet that operates under
//! Solana's existing P2P and RPC.
//!
//! The purpose for this is to allow PythNet to have a fast and reliable gossip layer for
//! generating state proofs and a reliable RPC layer for offering a distributed price service to
//! clients, as well as proof subscriptions.
//!
//! This is currently experimental and is meant to act as a possible alternative to Pyth's existing
//! price service.

use libp2p::Multiaddr;

use {
    anyhow::{
        anyhow,
        Result,
    },
    dashmap::DashMap,
    futures::{
        channel::mpsc::Receiver,
        SinkExt,
    },
    geyser::AccountUpdate,
    network::PythMessage,
    std::{
        net::SocketAddr,
        path::PathBuf,
        sync::Arc,
        time::Duration,
    },
    structopt::StructOpt,
    tokio::{
        spawn,
        time::sleep,
    },
};

mod geyser;
mod network;

/// An Accumulator itself is a 32-byte array.
pub type Accumulator = AccountUpdate;

/// A signature is a 64-byte array, but due to serde limitations we need to split it into two.
pub type Signature = [u8; 64];

/// A map of Accumulators to their corresponding multi-signatures.
pub type AccumulatorSig = Arc<DashMap<Accumulator, MultiSignature>>;

/// For experimentation purposes, we use a Wormhole-style multi-signature for Solana Accumulators.
/// This allows signing messages using the Validator's instead of the Guardians.
pub type MultiSignature = [Option<Signature>; MULTI_SIGNATURE_SIZE];

/// The number of signatures required to finalize an Accumulator when using Wormhole-style
/// multi-signatures.
pub const MULTI_SIGNATURE_SIZE: usize = 19;

/// Handler for LibP2P messages.
///
/// This function handles anything that might be gossiped between nodes. In particular it
/// performs the following actions:
///
/// - Sign and broadcast a new Accumulator.
/// - Gossip Geyser account updates to other nodes.
/// - Gossip ValidatorSet updates to other nodes.
///
fn handle_message(accumulators: AccumulatorSig, message: Vec<u8>) -> Result<()> {
    let message = serde_cbor::from_slice::<PythMessage>(&message)?;

    // Given a Message, update the AccumulatorMap to keep track of signatures for each accumulator.
    // DashSet takes care of deduplication.
    match message {
        PythMessage::Accumulator {
            account_update,
            signature,
        } => {
            // Update (or insert if missing) the DashSet for this Accumulator.
            {
                let mut current = accumulators
                    .entry(account_update.clone())
                    .or_insert_with(MultiSignature::default);

                // Append the signature to the DashSet entry. Place it in the first `None` slot.
                for i in 0..current.len() {
                    if current[i].is_none() {
                        current[i] = Some(signature);
                        break;
                    }
                }
            }

            {
                // Check at least 2/3 signatures have been received.
                let count = accumulators
                    .get(&account_update)
                    .ok_or(anyhow!("Accumulator Missing"))?
                    .iter()
                    .filter(|x| x.is_some())
                    .count();

                // Serialize the accumulator and base58 encode the bytes.
                let serialized = serde_cbor::to_vec(&account_update)?;
                let serialized = bs58::encode(serialized).into_string();

                // Check for a 2/3 majority.
                if count == 1 {
                    //MULTI_SIGNATURE_SIZE * 2 / 3 {
                    println!("Finalized {serialized}!");
                }
            }
        }

        PythMessage::ValidatorSet(_) => {
            log::info!("Received ValidatorSet");
        }
    };

    Ok(())
}

/// StructOpt definitions that provides the following arguments and commands:
///
/// - `--help`     -- Prints help information.
/// - `--version`  -- Prints version information.
///
/// Command: `run`
///
/// - `--id`       -- A Path to a protobuf encoded secp256k1 private key.
/// - `--wormhole` -- URI to the Wormhole RPC endpoint.
/// - `--rpc-addr` -- The address and port to bind the RPC server to.
/// - `--p2p-addr` -- The address and port to bind the P2P server to.
/// - `--p2p-peer` -- A bootstrapping peer to join the cluster. This can be specified multiple times.
///
/// Command: `keygen`
/// - `--output`   -- The path to write the generated key to.
///
#[derive(StructOpt, Debug)]
#[structopt(name = "pythnet", about = "PythNet")]
enum Options {
    /// Run the PythNet application.
    Run {
        /// A Path to a protobuf encoded secp256k1 private key.
        #[structopt(short, long)]
        id: PathBuf,

        /// URI to the Wormhole RPC endpoint.
        #[structopt(short, long)]
        wormhole: String,

        /// The address and port to bind the RPC server to.
        #[structopt(long, default_value = "127.0.0.1:33999")]
        rpc_addr: SocketAddr,

        /// The address and port to bind the P2P server to.
        #[structopt(long, default_value = "/ip4/127.0.0.1/tcp/34000")]
        p2p_addr: Multiaddr,

        /// A bootstrapping peer to join the cluster. This can be specified multiple times.
        #[allow(dead_code)]
        #[structopt(long)]
        p2p_peer: Vec<SocketAddr>,
    },

    /// Generate a new keypair.
    Keygen {
        /// The path to write the generated key to.
        #[structopt(short, long)]
        output: PathBuf,
    },
}

/// Initialize the Application. This can be invoked either by real main, or by the Geyser plugin.
async fn init(update_channel: Receiver<AccountUpdate>) -> anyhow::Result<()> {
    log::info!("Initializing PythNet...");

    // Parse the command line arguments with StructOpt, will exit automatically on `--help` or
    // with invalid arguments.
    match Options::from_args() {
        Options::Run {
            id,
            wormhole,
            rpc_addr,
            p2p_addr,
            p2p_peer: _,
        } => {
            log::info!("Starting PythNet...");

            // Load the private identity Key which is stored in byte form on disk.
            let id = network::read_identity(id).await?;

            // A thread-safe lookup table from Accumulators to their corresponding
            // multi-signatures. Shared between P2P and RPC contexts.
            let accumulators = Arc::new(dashmap::DashMap::new());

            // Spawn the RPC server.
            log::info!("Starting RPC server on {}", rpc_addr);
            spawn(network::spawn_rpc(
                accumulators.clone(),
                wormhole,
                rpc_addr.to_string(),
            ));

            // Spawn the P2P layer.
            log::info!("Starting P2P server on {}", p2p_addr);
            spawn(network::spawn_p2p(
                id,
                update_channel,
                handle_message,
                accumulators,
                p2p_addr.to_string().parse()?,
            ));

            tokio::signal::ctrl_c().await?;
        }

        Options::Keygen { output } => {
            network::write_new_identity(output).await?;
        }
    }

    Ok(())
}

/// Main entrypoint for the application. This is if compiled as a standalone application. The other
/// entrypoint is the Geyser plugin, which invokes `init`. See [ref:geyser_init].
#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    // Generate a stream of fake AccountUpdates when run in binary mode.
    let (mut tx, rx) = futures::channel::mpsc::channel(1);

    spawn(async move {
        let mut data = 0u32;

        loop {
            sleep(Duration::from_millis(200)).await;

            // Increment Data & serialize into a [0; 32].
            let data = {
                data += 1;
                let mut data = data.to_be_bytes().to_vec();
                data.resize(32, 0);
                data
            };

            let _ = SinkExt::send(
                &mut tx,
                AccountUpdate {
                    addr: [0; 32],
                    data,
                },
            )
            .await;
        }
    });

    if let Err(result) = init(rx).await {
        eprintln!("{}", result.backtrace());
        for cause in result.chain() {
            eprintln!("{cause}");
        }
    }

    Ok(())
}
