use {
    crate::AccumulatorSig,
    axum::{
        extract::ws::WebSocketUpgrade,
        response::Response,
        Extension,
    },
    typescript_type_def::TypeDef,
    // yerpc::{
    //     rpc,
    //     RpcClient,
    //     RpcSession,
    // },
};

#[derive(Debug, TypeDef, serde::Serialize, serde::Deserialize)]
struct AccumulatorRequest {
    accumulator: String,
}

struct Api {
    map: AccumulatorSig,
}

// #[rpc(all_positional)]
// impl Api {
//     async fn accumulator(&self, req: AccumulatorRequest) -> anyhow::Result<String> {
//         let acc = bs58::decode(req.accumulator).into_vec()?;
//         let acc = <[u8; 32]>::try_from(acc.as_slice())?;
//         let acc = *self.map.get(&acc).ok_or(anyhow!("Accumulator Not Found"))?;
//         let acc = serde_json::to_string(&acc)?;
//         Ok(acc)
//     }
// }

// Handle incoming WebSocket connections, the method hands over a new WebSocket to YeRPC to handle
// JSON-RPC. See `Api` for method implementations.
pub async fn handler(ws: WebSocketUpgrade, Extension(map): Extension<AccumulatorSig>) -> Response {
    // let (client, out) = RpcClient::new();
    // let session = RpcSession::new(client, Api { map });
    // yerpc::axum::handle_ws_rpc(ws, out, session).await
    unimplemented!()
}
