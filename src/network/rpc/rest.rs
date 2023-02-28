// This file implements a REST service for the Price Service. This is a mostly direct copy of the
// TypeScript implementation in the `pyth-crosschain` repo. It uses `axum` as the web framework and
// `tokio` as the async runtime.
use {
    super::VaaCache,
    anyhow::Result,
    axum::{
        extract::{
            Query,
            State,
        },
        response::IntoResponse,
        Json,
    },
};

const WORMHOLE_RPC: &str = "https://figment-wormhole-rpc.herokuapp.com";

/// This method queries the Wormhole RPC for the latest VAA's for a given Price ID and
/// adds them to the VAA cache.
pub async fn update_wormhole_vaas(mut vaa_cache: VaaCache, price_feed_id: String) -> Result<()> {
    // Fetch VAA's from one minute ago.
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64
        - 60;

    /// This type captures the Vaa structure returned by the Wormhole API.
    #[derive(serde::Deserialize)]
    struct WormholeVaa {
        pub publish_time: i64,
        pub vaa:          String,
    }

    // Pull recent VAA's and insert them into the VAA cache.
    reqwest::Client::new()
        .get(format!(
            "{WORMHOLE_RPC}/vaa?id={price_feed_id}&publishTime={timestamp}&cluster=pythnet"
        ))
        .send()
        .await?
        .json::<Vec<WormholeVaa>>()
        .await?
        .into_iter()
        .for_each(|vaa: WormholeVaa| {
            vaa_cache.add(price_feed_id.clone(), vaa.publish_time, vaa.vaa)
        });

    Ok(())
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct LatestVaaQueryParams {
    ids: Vec<String>,
}

/// REST endpoint /latest_price_feeds?ids[]=...&ids[]=...&ids[]=...
pub async fn latest_vaas(
    State(state): State<super::State>,
    Query(params): Query<LatestVaaQueryParams>,
) -> Result<impl IntoResponse, std::convert::Infallible> {
    update_wormhole_vaas(state.vaa_cache.clone(), params.ids[0].clone())
        .await
        .unwrap();
    Ok(Json(state.vaa_cache.latest_for_ids(params.ids)))
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct LastAccsQueryParams {
    id: String,
}

/// REST endpoint /accumulator?id=...
pub async fn accumulator(
    State(state): State<super::State>,
    Query(params): Query<LastAccsQueryParams>,
) -> Result<impl IntoResponse, std::convert::Infallible> {
    let hex = hex::decode(params.id).unwrap();
    let cbr = serde_cbor::from_slice(&hex).unwrap();
    Ok(Json(
        state
            .accumulator_map
            .get(&cbr)
            .unwrap()
            .into_iter()
            .map(|x| hex::encode(x.unwrap_or([0; 64])))
            .collect::<Vec<String>>(),
    ))
}

// This function implements the `/live` endpoint. It returns a `200` status code. This endpoint is
// used by the Kubernetes liveness probe.
pub async fn live() -> Result<impl IntoResponse, std::convert::Infallible> {
    Ok(())
}

// This is the index page for the REST service. It will list all the available endpoints.
// TODO: Dynamically generate this list if possible.
pub async fn index() -> impl IntoResponse {
    Json(["/accumulator", "/latest_vaas", "/live", "/ws", "/"])
}
