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

use {
    anyhow::Result,
    solana_geyser_plugin_interface::geyser_plugin_interface::{
        GeyserPlugin, GeyserPluginError, ReplicaAccountInfoVersions,
    },
    std::{
        collections::HashSet,
        io::Write,
        sync::{
            mpsc::{channel, Sender},
            Mutex,
        },
    },
};

/// A PythNet AccountUpdate event containing a 32-byte Pubkey and the updated account data.
#[derive(Debug, Clone, Eq, Hash, PartialEq)]
pub struct AccountUpdate {
    addr: [u8; 32],
    data: Vec<u8>,
}

#[derive(Debug)]
struct PythNetPlugin {
    tx: Option<Mutex<Sender<AccountUpdate>>>,
}

lazy_static::lazy_static! {
    static ref ACCOUNT_FILTER: HashSet<[u8; 32]> = {
        let mut set = HashSet::new();
        set.insert(bs58::decode("SysvarAccumu1ator11111111111111111111111111").into_vec().unwrap().try_into().unwrap());
        set.insert(bs58::decode("3DCXkGuYjg2pGuG4CMdVwp5wAVNm7fdkK353DfxvCrH6").into_vec().unwrap().try_into().unwrap());
        set
    };
}

/// Implement the Solana Geyser Plugin interface.
impl GeyserPlugin for PythNetPlugin {
    fn name(&self) -> &'static str {
        "PythNet"
    }

    fn on_load(&mut self, _config: &str) -> Result<(), GeyserPluginError> {
        // Initialize Env Logger Context.
        env_logger::init();

        log::info!("PythNet: Plugin Loaded");

        // Setup a channel to forward account updates to the IPC pipe.
        let (tx, rx) = channel();
        self.tx = Some(Mutex::new(tx));

        // This handler asynchronously runs in the background to receive & write account updates to
        // the IPC. The reason for this rather than writing directly in `update_account` is because
        // there is no nice synchronous library for writing to a Unix Domain Socket. Rather than
        // use `libc` and `RawFd` directly, we use the `tokio` library which provides a nice async
        // interface which is easy to reason about.

        // Open a Domain socket using the standard rust stdlib.
        let mut ipc = {
            use std::os::unix::net::UnixStream;
            let path = std::path::Path::new("pythnet.sock");
            UnixStream::connect(path)?
        };

        std::thread::spawn(move || {
            // Wait for updates from the Geyser plugin and write them to the Socket pipe.
            loop {
                let update = rx.recv().unwrap();

                // Write the update into a buffer so the IPC write is atomic.
                let mut buf = Vec::new();
                let mut cur = std::io::Cursor::new(&mut buf);
                let _ = cur.write_all(&update.addr);
                let _ = cur.write_all(&update.data.len().to_be_bytes());
                let _ = cur.write_all(&update.data);

                // When failing, we log but don't retry. This is because if the remote end of the
                // pipe is closed, we don't want to block the Geyser plugin. We may need to revisit
                // this if we need to avoid any data loss.
                if let Err(e) = ipc.write_all(&buf) {
                    log::error!("PythNet: Failed to write Update: {}", e);
                }
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
        };

        // Specifically match only the accounts we care about. Note bs58 is a relatively slow
        // encoding and we should come back and remove this.
        if ACCOUNT_FILTER.contains(address) {
            log::info!(
                "PythNet: Matched Account: {}",
                bs58::encode(address).into_string()
            );

            if let Some(ipc_tx) = &mut self.tx {
                ipc_tx
                    .lock()
                    .unwrap()
                    .send(AccountUpdate {
                        addr: address.try_into().map_err(|_| {
                            GeyserPluginError::Custom("PythNet: Invalid Address".into())
                        })?,
                        data: data.to_owned(),
                    })
                    .map_err(|e| {
                        GeyserPluginError::Custom(
                            format!("PythNet: Account Update Channel Closed {}", e).into(),
                        )
                    })?;
            }
        }

        Ok(())
    }
}

#[no_mangle]
#[allow(improper_ctypes_definitions)]
pub unsafe extern "C" fn _create_plugin() -> *mut dyn GeyserPlugin {
    let plugin = PythNetPlugin { tx: None };
    let plugin: Box<dyn GeyserPlugin> = Box::new(plugin);
    Box::into_raw(plugin)
}
