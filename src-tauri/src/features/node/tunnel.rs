//! Local SSH tunnels that expose a remote node's loopback A2A endpoint.
//!
//! The node only listens on `127.0.0.1` on the server, so the desktop reaches
//! it through the user's own OpenSSH client: `ssh -N -L 127.0.0.1:<local>:…:8766
//! <host>`. This mirrors the remote-Vault path (system OpenSSH, no new daemon)
//! and reuses `~/.ssh/config` + agent/key auth for free.

use std::collections::HashMap;
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeTunnel {
    /// `http://127.0.0.1:<port>` — the node's base URL through the tunnel.
    pub local_url: String,
    pub ssh_host: String,
    pub remote_port: u16,
}

fn registry() -> &'static Mutex<HashMap<String, Child>> {
    static REG: OnceLock<Mutex<HashMap<String, Child>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

fn free_port() -> std::io::Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}

/// Open a tunnel and wait until the forwarded port accepts connections.
pub fn open(ssh_host: &str, remote_port: u16) -> Result<NodeTunnel, String> {
    let ssh_host = ssh_host.trim();
    if ssh_host.is_empty() {
        return Err("ssh host is required".to_string());
    }
    let local_port = free_port().map_err(|e| e.to_string())?;
    let mut child = Command::new("ssh")
        .args([
            "-N",
            "-o",
            "BatchMode=yes",
            "-o",
            "ExitOnForwardFailure=yes",
            "-o",
            "ConnectTimeout=8",
        ])
        .arg("-L")
        .arg(format!("127.0.0.1:{local_port}:127.0.0.1:{remote_port}"))
        .arg(ssh_host)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("failed to start ssh: {e}"))?;

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!("ssh tunnel to {ssh_host} failed ({status})"));
        }
        match TcpStream::connect(("127.0.0.1", local_port)) {
            Ok(_) => break,
            Err(_) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(120));
            }
            Err(error) => {
                let _ = child.kill();
                return Err(format!("ssh tunnel to {ssh_host} not ready: {error}"));
            }
        }
    }

    let local_url = format!("http://127.0.0.1:{local_port}");
    registry()
        .lock()
        .map_err(|_| "tunnel lock poisoned".to_string())?
        .insert(local_url.clone(), child);
    Ok(NodeTunnel {
        local_url,
        ssh_host: ssh_host.to_string(),
        remote_port,
    })
}

/// Close the tunnel registered under `local_url`. Returns whether one existed.
pub fn close(local_url: &str) -> bool {
    if let Ok(mut map) = registry().lock() {
        if let Some(mut child) = map.remove(local_url) {
            let _ = child.kill();
            let _ = child.wait();
            return true;
        }
    }
    false
}

/// Kill every open tunnel (called from the app exit handler).
pub fn close_all() {
    if let Ok(mut map) = registry().lock() {
        for (_, mut child) in map.drain() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
