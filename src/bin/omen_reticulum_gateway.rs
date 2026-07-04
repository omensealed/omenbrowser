use std::time::Duration;

use rand_core::OsRng;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut listen = String::from("127.0.0.1:42422");
    let mut network_name: Option<String> = None;
    let mut passphrase: Option<String> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--listen" => {
                listen = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--listen requires HOST:PORT"))?;
            }
            "--network-name" => {
                network_name = Some(
                    args.next()
                        .ok_or_else(|| anyhow::anyhow!("--network-name requires a value"))?,
                );
            }
            "--passphrase" => {
                passphrase = Some(
                    args.next()
                        .ok_or_else(|| anyhow::anyhow!("--passphrase requires a value"))?,
                );
            }
            "-h" | "--help" => {
                eprintln!(
                    "usage: omen-reticulum-gateway [--listen HOST:PORT] [--network-name NAME] [--passphrase SECRET]"
                );
                return Ok(());
            }
            other => anyhow::bail!("unknown argument: {other}"),
        }
    }

    let identity = rns_transport::identity::PrivateIdentity::new_from_rand(OsRng);
    let mut config =
        reticulum_rs::runtime::TransportConfig::new("omen-reticulum-gateway", &identity, true);
    config.set_retransmit(true);
    let transport = reticulum_rs::runtime::Transport::new(config);
    let manager = transport.iface_manager();
    let server = rns_transport::iface::tcp_server::TcpServer::new(listen.clone(), manager.clone());
    let status = server.runtime_status_handle();
    {
        let mut guard = manager.lock().await;
        let context = guard.new_context(server);
        let iface = *context.channel.address();
        if network_name.is_some() || passphrase.is_some() {
            let shared = rns_transport::iface::InterfaceSharedConfig {
                ifac_size: Some(16),
                network_name,
                passphrase,
                ..rns_transport::iface::InterfaceSharedConfig::default()
            };
            if !guard.set_shared_config(iface, shared) {
                anyhow::bail!("failed to configure gateway IFAC");
            }
        }
        tokio::spawn(rns_transport::iface::tcp_server::TcpServer::spawn(context));
        println!(
            "gateway ready listen={} transport=true iface={}",
            listen,
            iface.to_hex_string()
        );
    }

    loop {
        let snapshot = status.to_json();
        let state = snapshot
            .get("listener_state")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown");
        let accepted = snapshot
            .get("accepted_connections")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let errors = snapshot
            .get("accept_errors")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        eprintln!("gateway status state={state} accepted={accepted} accept_errors={errors}");
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
}
