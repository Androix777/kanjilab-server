use std::sync::{Mutex, OnceLock};

use kameo::Actor;
use tokio::{
    net::TcpListener,
    runtime::{Builder, Runtime},
    sync::broadcast,
};

use crate::game_actor::{GameActor, NewClient};

struct ServerState {
    stop_tx: broadcast::Sender<()>,
    _rt: Runtime,
}

static STATE: OnceLock<Mutex<Option<ServerState>>> = OnceLock::new();

pub fn call_launch_server(port: impl Into<String>) -> Result<(), String> {
    let addr = format!("127.0.0.1:{}", port.into());

    let lock = STATE.get_or_init(|| Mutex::new(None));
    let mut guard = lock.lock().unwrap();
    if guard.is_some() {
        return Err("server already running".into());
    }

    let rt = Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;

    let (stop_tx, mut stop_rx) = broadcast::channel::<()>(1);

    let handle = rt.handle().clone();
    handle.spawn(async move {
        let game = GameActor::spawn(());
        let listener = TcpListener::bind(&addr).await.expect("bind tcp listener");

        loop {
            tokio::select! {
                Ok((stream, _)) = listener.accept() => {
                    let g = game.clone();
                    tokio::spawn(async move { g.tell(NewClient(stream)).await.ok(); });
                }
                _ = stop_rx.recv() => {
                    tracing::info!("server on {addr} shutting down");
                    break;
                }
            }
        }
    });

    *guard = Some(ServerState { stop_tx, _rt: rt });
    Ok(())
}

pub fn call_stop_server() -> Result<(), String> {
    let lock = STATE
        .get()
        .ok_or_else(|| "server was never started".to_string())?;
    let mut guard = lock.lock().unwrap();

    let ServerState { stop_tx, .. } = guard
        .take()
        .ok_or_else(|| "server is not running".to_string())?;

    let _ = stop_tx.send(());
    Ok(())
}
