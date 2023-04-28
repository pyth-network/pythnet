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
        os::unix::net::UnixListener,
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

        let socket = {
            let path = std::path::Path::new("pythnet.sock");
            if path.exists() {
                std::fs::remove_file(path)?;
            }
            UnixListener::bind(path)?
        };

        std::thread::spawn(move || {
            // Open a Unix Domain Socket to write updates to, the other side of this channel can be
            // any program interested in program updates, but is mainly intended to be used by a
            // hermes instance.
            loop {
                match socket.accept() {
                    Err(e) => {
                        log::error!("PythNet: Failed to accept connection: {}", e);
                        std::thread::sleep(std::time::Duration::from_secs(1));
                    }

                    Ok((mut stream, _addr)) => {
                        loop {
                            let update = rx.recv().unwrap();

                            // Write the update into a buffer so the IPC write is atomic.
                            let mut buf = Vec::new();
                            let mut cur = std::io::Cursor::new(&mut buf);
                            let _ = cur.write_all(&update.addr);
                            let _ = cur.write_all(&update.data.len().to_be_bytes());
                            let _ = cur.write_all(&update.data);

                            // The assumption here is that we have a valid Unix Domain Socket connection to
                            // write to, if this fails, it is likely the other side of the pipe has closed.
                            // We break out of the loop here to attempt to wait for new connections. During
                            // this period we will miss writes to the pipe and the consuming side should
                            // consider what to do about data loss.
                            if let Err(e) = stream.write_all(&buf) {
                                log::error!("PythNet: Failed to write Update: {}", e);
                                break;
                            }
                        }
                    }
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
