//! A Geyser plugin that writes PythNet Accumulator updates to a Unix Domain Socket.
//!
//! This component is meant to be coupled with the Pyth node software found in the
//! `pyth-crosschain` repository. This plugin watches for updates to the Accumulator
//! sysvar and writes the updates to a Unix Domain Socket which is then read by the
//! Pyth node software.
//!
//! All logic related to processing these account updates, whether that be proving
//! prices of signing statements, is handled by the Pyth node software. This plugin
//! is therefore intended to be kept as simple (and fast) as possible.
//!
//! In the future it may be desirable to have this plugin write updates to accounts
//! other than the Accumulator.

use tokio::{sync::mpsc::{Sender, Receiver}, io::AsyncWriteExt};
use {
    anyhow::Result,
    solana_geyser_plugin_interface::geyser_plugin_interface::{
        GeyserPlugin, GeyserPluginError, ReplicaAccountInfoVersions,
    },
    std::collections::HashSet,
};

/// A PythNet AccountUpdate event containing a 32-byte Pubkey and the updated account data.
#[derive(Debug, Clone, Eq, Hash, PartialEq)]
pub struct AccountUpdate {
    addr: [u8; 32],
    data: Vec<u8>,
}

#[derive(Debug)]
struct PythNetPlugin {
    ipc_tx: Sender<AccountUpdate>,
}

lazy_static::lazy_static! {
    static ref ACCOUNT_FILTER: HashSet<[u8; 32]> = {
        let mut set = HashSet::new();
        set.insert(bs58::decode("SysvarAccum111111111111111111111111").into_vec().unwrap().try_into().unwrap());
        set
    };
}

/// Implement the Solana Geyser Plugin interface.
impl GeyserPlugin for PythNetPlugin {
    fn name(&self) -> &'static str {
        "PythNet"
    }

    fn on_load(&mut self, _config: &str) -> Result<(), GeyserPluginError> {
        log::info!("PythNet Plugin Loaded");

        // The main application logic requires the tokio runtime to be running. Which it won't be
        // by default given the Geyser plugin architecture.
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;

        // Setup a channel to forward account updates to the IPC pipe.
        let (tx, rx) = tokio::sync::mpsc::channel(1024);
        self.ipc_tx = tx;

        // This handler asynchronously runs in the background to receive & write account updates to
        // the IPC. The reason for this rather than writing directly in `update_account` is because
        // there is no nice synchronous library for writing to a Unix Domain Socket. Rather than
        // use `libc` and `RawFd` directly, we use the `tokio` library which provides a nice async
        // interface which is easy to reason about.
        async fn handler(mut rx: Receiver<AccountUpdate>) -> anyhow::Result<()> {
            // Open a UNIX pipe.
            let mut ipc = tokio::net::unix::pipe::OpenOptions::new()
                .open_sender("pythnet.pipe")?;

            // Wait for updates from the Geyser plugin and write them to the IPC pipe.
            while let Some(update) = rx.recv().await {
                // Write the update into a buffer so the IPC write is atomic.
                let mut buf = Vec::new();
                let mut cur = std::io::Cursor::new(&mut buf);
                cur.write_all(&update.addr).await?;
                cur.write_u32(update.data.len() as u32).await?;
                cur.write_all(&update.data).await?;

                // When failing, we log but don't retry. This is because if the remote end of the
                // pipe is closed, we don't want to block the Geyser plugin. We may need to revisit
                // this if we need to avoid any data loss.
                if let Err(e) = ipc.write_all(&buf).await {
                    log::error!("Failed to write Update: {}", e);
                }
            }

            Ok(())
        }

        // Spawn a task to write account updates to the IPC pipe.
        rt.spawn(async move {
            if let Err(e) = handler(rx).await {
                log::error!("Fatal PythNet Plugin Error: {}", e);
            }
        });

        Ok(())
    }

    fn update_account(
        &mut self,
        account: ReplicaAccountInfoVersions,
        _slot: u64,
        _is_startup: bool,
    ) -> Result<(), GeyserPluginError> {
        // Extract Pubkey/Data from whatever account version we are given.
        let (address, data) = match account {
            ReplicaAccountInfoVersions::V0_0_1(account) => (account.pubkey, account.data),
            ReplicaAccountInfoVersions::V0_0_2(account) => (account.pubkey, account.data),
        };

        // Specifically match only the accounts we care about. Note bs58 is a relatively slow
        // encoding and we should come back and remove this.
        if ACCOUNT_FILTER.contains(address) {
            self.ipc_tx
                .try_send(AccountUpdate {
                    addr: address.try_into().map_err(|_| GeyserPluginError::Custom("Invalid Address".into()))?,
                    data: data.to_owned(),
                })
                .map_err(|_| GeyserPluginError::Custom("Account Update Channel Closed".into()))?;
        }

        Ok(())
    }
}
