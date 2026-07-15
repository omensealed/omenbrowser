use std::io::Read;
use std::path::Path;
use std::time::Duration;

use rand_core::OsRng;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let resolved_args = resolve_passphrase_args(std::env::args().skip(1).collect())?;
    let mut args = resolved_args.into_iter();
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
                    "usage: omen-reticulum-gateway [--listen HOST:PORT] [--network-name NAME] [--passphrase-file PATH|--passphrase-stdin|--passphrase-prompt]\n--passphrase SECRET is deprecated because argv may be visible to other processes"
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

fn resolve_passphrase_args(args: Vec<String>) -> anyhow::Result<Vec<String>> {
    let mut resolved = Vec::with_capacity(args.len());
    let mut args = args.into_iter();
    let mut source_seen = false;
    while let Some(arg) = args.next() {
        let value = match arg.as_str() {
            "--passphrase" => {
                ensure_single_source(&mut source_seen)?;
                eprintln!(
                    "warning: --passphrase exposes secrets in process listings; use a safe passphrase source"
                );
                Some(validate_passphrase(args.next().ok_or_else(|| {
                    anyhow::anyhow!("--passphrase requires a value")
                })?)?)
            }
            "--passphrase-file" => {
                ensure_single_source(&mut source_seen)?;
                let path = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--passphrase-file requires a path"))?;
                Some(read_passphrase_file(Path::new(&path))?)
            }
            "--passphrase-stdin" => {
                ensure_single_source(&mut source_seen)?;
                Some(read_passphrase(std::io::stdin().lock())?)
            }
            "--passphrase-prompt" => {
                ensure_single_source(&mut source_seen)?;
                Some(validate_passphrase(rpassword::prompt_password(
                    "IFAC passphrase: ",
                )?)?)
            }
            _ => None,
        };
        if let Some(value) = value {
            resolved.extend(["--passphrase".into(), value]);
        } else {
            resolved.push(arg);
        }
    }
    Ok(resolved)
}

fn ensure_single_source(seen: &mut bool) -> anyhow::Result<()> {
    if std::mem::replace(seen, true) {
        anyhow::bail!("choose exactly one passphrase source");
    }
    Ok(())
}

fn read_passphrase_file(path: &Path) -> anyhow::Result<String> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        anyhow::bail!("passphrase file must be a regular non-symlink file");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            anyhow::bail!("passphrase file permissions must not allow group or other access");
        }
    }
    read_passphrase(std::fs::File::open(path)?)
}

fn read_passphrase(reader: impl Read) -> anyhow::Result<String> {
    let mut bytes = Vec::new();
    reader.take(4097).read_to_end(&mut bytes)?;
    if bytes.len() > 4096 {
        anyhow::bail!("passphrase input exceeds 4096 bytes");
    }
    let value = String::from_utf8(bytes)?;
    validate_passphrase(value.trim_end_matches(['\r', '\n']).to_owned())
}

fn validate_passphrase(value: String) -> anyhow::Result<String> {
    if value.is_empty() || value.contains('\0') {
        anyhow::bail!("passphrase must be non-empty and contain no NUL bytes");
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_sources_are_bounded_and_exclusive() {
        assert_eq!(
            read_passphrase(std::io::Cursor::new(b" secret \r\n")).unwrap(),
            " secret "
        );
        assert!(read_passphrase(std::io::Cursor::new(vec![b'x'; 4097])).is_err());
        assert!(resolve_passphrase_args(vec![
            "--passphrase".into(),
            "one".into(),
            "--passphrase-prompt".into(),
        ])
        .is_err());
    }
}
