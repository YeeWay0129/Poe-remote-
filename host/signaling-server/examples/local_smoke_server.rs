use host_core::config::HostConfig;
use signaling_server::{SignalingBindConfig, SignalingServer, spawn_plain_ws_server};
use std::net::SocketAddr;
use std::thread;
use std::time::Duration;

const DEFAULT_PAIRING_PASSWORD_HASH: &str =
    "c53fb561532b1638f6ce48c7992eb69eda7780a3af1b0205d40342b922a77c19";

fn main() -> std::io::Result<()> {
    let server = SignalingServer::new(HostConfig::new(DEFAULT_PAIRING_PASSWORD_HASH));
    let runtime = spawn_plain_ws_server(
        server,
        SignalingBindConfig {
            bind_addr: SocketAddr::from(([0, 0, 0, 0], 7443)),
            ..SignalingBindConfig::default()
        },
    )?;

    println!("local smoke signaling: ws://{}/signaling", runtime.bind_addr());

    loop {
        thread::sleep(Duration::from_secs(60));
    }
}
