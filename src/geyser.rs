use {
    futures::channel::mpsc::Sender,
    solana_geyser_plugin_interface::geyser_plugin_interface::{
        GeyserPlugin,
        GeyserPluginError,
        ReplicaAccountInfoVersions,
    },
    std::collections::HashSet,
};

/// A Structure exposing an account update for other layers to consume. These are to be sent over a
/// channel to the P2P layer.
#[derive(Eq, PartialEq, Hash, Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct AccountUpdate {
    pub addr: [u8; 32],
    pub data: Vec<u8>,
}

#[derive(Debug)]
struct PythPlugin {
    /// A Channel to forward Geyser account updates out of the plugin and into the P2P layer.
    account_update_channel: Sender<AccountUpdate>,

    /// Geyser IPC channel, used to forward data from the update function to the loop that writes
    /// updates to the IPC pipe.
    ipc_channel: Sender<AccountUpdate>,
}

// Same as above but as a lazy static.
lazy_static::lazy_static! {
    static ref ACCOUNT_FILTER: HashSet<&'static str> = {
        let mut set = HashSet::new();
        set.insert("Sysvar1n111111111111111111111111111");
        set.insert("SysvarC1ock111111111111111111111111");
        set.insert("SysvarF1111111111111111111111111111");
        set
    };
}

/// Implement the Solana Geyser Plugin interface.
impl GeyserPlugin for PythPlugin {
    fn name(&self) -> &'static str {
        "PythNet"
    }

    fn on_load(&mut self, _config: &str) -> Result<(), GeyserPluginError> {
        use crate::init;

        log::info!("PythNet Plugin Loaded");

        // The main application logic requires the tokio runtime to be running. Which it won't be
        // by default given the Geyser plugin architecture.
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;

        // Setup a channel to forward account updates to the main application logic.
        let (tx, p2p_rx) = futures::channel::mpsc::channel(1);
        self.account_update_channel = tx;

        // Setup a channel to forward account updates to the IPC pipe.
        let (tx, mut ipc_rx) = futures::channel::mpsc::channel(1);
        self.ipc_channel = tx;

        // Spawn a task to write account updates to the IPC pipe.
        rt.spawn(async move {
            let ipc = tokio::net::unix::pipe::OpenOptions::new().open_sender("pythnet.pipe").unwrap();

            // Listen to the IPC channel for account updates and write them to the IPC pipe.
            while let Some(update) = ipc_rx.try_next().unwrap() {
                // Try to write data, this may still fail with `WouldBlock`.
                match ipc.try_write(&update.data) {
                    Ok(_) => {}
                    Err(e) => {
                        match e.kind() {
                            std::io::ErrorKind::WouldBlock => log::warn!("Account Update Pipe Full"),
                            _                              => log::error!("Account Update Pipe Error: {}", e),
                        }
                    }
                }
            }
        });

        // Spawn the main application logic. Note that this must not block, as on_load must return
        // for the plugin to successfully load, so we are using tokio::spawn to run this in the
        // background.
        //
        // [tag:geyser_init]
        rt.spawn(init(p2p_rx));

        Ok(())
    }

    fn update_account(
        &mut self,
        account: ReplicaAccountInfoVersions,
        slot: u64,
        is_startup: bool,
    ) -> Result<(), GeyserPluginError> {
        // Extract Pubkey/Data from whatever account version we are given.
        let (address, data) = match account {
            ReplicaAccountInfoVersions::V0_0_1(account) => (account.pubkey, account.data),
            ReplicaAccountInfoVersions::V0_0_2(account) => (account.pubkey, account.data),
        };

        // Specifically match only the accounts we care about. Note bs58 is a relatively slow
        // encoding and we should come back and remove this.
        if ACCOUNT_FILTER.contains(&bs58::encode(address).into_string().as_str()) {
            log::info!("ACCOUNT_UPDATE {} {} {}b", slot, is_startup, data.len(),);
            self.account_update_channel
                .try_send(AccountUpdate {
                    addr: address.try_into().unwrap(),
                    data: data.to_owned(),
                }).map_err(|_| GeyserPluginError::Custom("Account Update Channel Closed".into()))?;
        }

        Ok(())
    }
}
