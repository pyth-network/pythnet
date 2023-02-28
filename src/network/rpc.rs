use {
    crate::AccumulatorSig,
    axum::{
        routing::get,
        Router,
    },
    dashmap::DashMap,
    std::sync::Arc,
};

mod rest;
mod websocket;

#[derive(Clone, Default)]
pub struct VaaCache(Arc<DashMap<String, Vec<(i64, String)>>>);

impl VaaCache {
    /// Add a VAA to the cache. Keeps the cache sorted by timestamp.
    fn add(&mut self, key: String, timestamp: i64, vaa: String) {
        self.remove_expired();
        let mut entry = self.0.entry(key).or_default();
        let key = entry.binary_search_by(|(t, _)| t.cmp(&timestamp)).unwrap();
        entry.insert(key, (timestamp, vaa));
    }

    /// Remove expired VAA's from the cache.
    fn remove_expired(&mut self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Scan for items older than now, remove, if the result is empty remove the key altogether.
        for mut item in self.0.iter_mut() {
            let (key, vaas) = item.pair_mut();
            vaas.retain(|(t, _)| t > &now);
            if vaas.is_empty() {
                self.0.remove(key);
            }
        }
    }

    /// For a given set of Price IDs, return the latest VAA for each Price ID.
    fn latest_for_ids(&self, ids: Vec<String>) -> Vec<(String, String)> {
        self.0
            .iter()
            .filter_map(|item| {
                if !ids.contains(item.key()) {
                    return None;
                }

                let (_, latest_vaa) = item.value().last()?;
                Some((item.key().clone(), latest_vaa.clone()))
            })
            .collect()
    }
}

#[derive(Clone)]
pub struct State {
    /// A Cache of VAA's that have been fetched from the Wormhole RPC.
    pub vaa_cache: VaaCache,

    /// The Accumulator Dashmap, containing the current verified Accumulator's.
    pub accumulator_map: AccumulatorSig,

    /// RPC Address for Wormhole
    pub wormhole: String,
}

impl State {
    fn new(accumulator_map: AccumulatorSig, wormhole: String) -> Self {
        Self {
            vaa_cache: VaaCache::default(),
            accumulator_map,
            wormhole,
        }
    }
}

/// This method provides a background service that responds to JSON-RPC requests. State between the
/// Geyser and P2P layers are synced through the DashMap state passed in from the application's main
/// entrypoint.
///
/// Currently this is based on Axum & YeRPC due to the simplicity and strong ecosystem support for
/// the packages they are based on.
pub async fn spawn_rpc(map: AccumulatorSig, wormhole: String, rpc_addr: String) {
    let cfg = State::new(map, wormhole);

    // Initialize Axum Router. Note the type here is a `Router<State>` due to the use of the
    // `with_state` method which replaces `Body` with `State` in the type signature.
    let app = Router::new();
    let app = app
        .route("/", get(rest::index))
        .route("/ws", get(websocket::handler))
        .route("/live", get(rest::live))
        .route("/accumulator", get(rest::accumulator))
        .route("/latest_vaas", get(rest::latest_vaas))
        .with_state(cfg.clone());

    // Binds the axum's server to the configured address and port. This is a blocking call and will
    // not return until the server is shutdown.
    axum::Server::bind(&rpc_addr.parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}
