use serde_json::json;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Public hosted "Minecraft" Discord Rich Presence Application ID (CraftPresence default).
/// Works out of the box without requiring users to register a developer app; users can also override it.
pub const DEFAULT_DISCORD_APP_ID: &str = "450485984333660181";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscordActivity {
    pub details: String,
    pub state: String,
    pub start_timestamp: u64,
    pub custom_client_id: Option<String>,
}

enum RpcMsg {
    Set(DiscordActivity),
    Clear,
    Shutdown,
}

struct DiscordRpcInner {
    tx: Sender<RpcMsg>,
}

impl Drop for DiscordRpcInner {
    fn drop(&mut self) {
        let _ = self.tx.send(RpcMsg::Shutdown);
    }
}

#[derive(Clone)]
pub struct DiscordRpcController {
    inner: Arc<DiscordRpcInner>,
}

impl DiscordRpcController {
    pub fn spawn(on_status: Arc<dyn Fn(bool, String) + Send + Sync>) -> Self {
        let (tx, rx) = mpsc::channel::<RpcMsg>();
        std::thread::Builder::new()
            .name("axylauncher-discord-rpc".to_string())
            .spawn(move || rpc_worker_loop(rx, on_status))
            .ok();
        Self {
            inner: Arc::new(DiscordRpcInner { tx }),
        }
    }

    pub fn set_launcher_presence(
        &self,
        enabled: bool,
        selected_inst: Option<(&str, &str, &str)>,
        custom_client_id: &str,
        start_ts: u64,
    ) {
        if !enabled {
            let _ = self.inner.tx.send(RpcMsg::Clear);
            return;
        }
        let (details, state) = match selected_inst {
            Some((name, mc, loader)) => (
                "В меню AxyLauncher".to_string(),
                format!("Выбрано: {} ({} • {})", name, mc, loader),
            ),
            None => (
                "В меню AxyLauncher".to_string(),
                "Выбор сборки Minecraft".to_string(),
            ),
        };
        let cid = custom_client_id.trim();
        let _ = self.inner.tx.send(RpcMsg::Set(DiscordActivity {
            details,
            state,
            start_timestamp: start_ts,
            custom_client_id: if cid.is_empty() {
                None
            } else {
                Some(cid.to_string())
            },
        }));
    }

    pub fn set_game_presence(
        &self,
        enabled: bool,
        inst_name: &str,
        mc_version: &str,
        loader: &str,
        custom_client_id: &str,
    ) {
        if !enabled {
            let _ = self.inner.tx.send(RpcMsg::Clear);
            return;
        }
        let now_ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let cid = custom_client_id.trim();
        let _ = self.inner.tx.send(RpcMsg::Set(DiscordActivity {
            details: format!("Играет: {}", inst_name),
            state: format!("Minecraft {} • {}", mc_version, loader),
            start_timestamp: now_ts,
            custom_client_id: if cid.is_empty() {
                None
            } else {
                Some(cid.to_string())
            },
        }));
    }

    #[allow(dead_code)]
    pub fn clear(&self) {
        let _ = self.inner.tx.send(RpcMsg::Clear);
    }
}

trait IpcStream: Read + Write + Send {
    fn set_nonblocking_mode(&mut self, nonblocking: bool);
}

#[cfg(unix)]
impl IpcStream for std::os::unix::net::UnixStream {
    fn set_nonblocking_mode(&mut self, nonblocking: bool) {
        let _ = self.set_nonblocking(nonblocking);
    }
}

#[cfg(windows)]
impl IpcStream for std::fs::File {
    fn set_nonblocking_mode(&mut self, _nonblocking: bool) {}
}

/// Enumerate all candidate Discord IPC socket paths on Linux (native, Flatpak, Vesktop, ArmCord, Snap, /tmp) & macOS.
pub fn candidate_unix_ipc_paths() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();

    if let Some(xdg) = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from) {
        dirs.push(xdg.clone());
        dirs.push(xdg.join("app/com.discordapp.Discord"));
        dirs.push(xdg.join("app/com.discordapp.DiscordCanary"));
        dirs.push(xdg.join("app/com.discordapp.DiscordPTB"));
        dirs.push(xdg.join("app/dev.vencord.Vesktop"));
        dirs.push(xdg.join("app/xyz.armcord.ArmCord"));
        dirs.push(xdg.join("app/io.github.spacingbat3.webcord"));
        dirs.push(xdg.join("app/io.github.equicord.equibop"));
        dirs.push(xdg.join("app/org.equicord.Equibop"));
        dirs.push(xdg.join("snap.discord"));
        dirs.push(xdg.join("snap.discord-canary"));
    }

    // Fallback to /run/user/<uid> on Linux if XDG_RUNTIME_DIR wasn't inherited
    if cfg!(target_os = "linux") {
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if let Some(rest) = line.strip_prefix("Uid:") {
                    if let Some(uid_str) = rest.split_whitespace().next() {
                        let run_user = PathBuf::from(format!("/run/user/{}", uid_str));
                        if !dirs.contains(&run_user) {
                            dirs.push(run_user.clone());
                            dirs.push(run_user.join("app/com.discordapp.Discord"));
                            dirs.push(run_user.join("app/dev.vencord.Vesktop"));
                            dirs.push(run_user.join("snap.discord"));
                        }
                    }
                    break;
                }
            }
        }
    }

    for env_key in ["TMPDIR", "TMP", "TEMP"] {
        if let Some(t) = std::env::var_os(env_key).map(PathBuf::from) {
            if !dirs.contains(&t) {
                dirs.push(t);
            }
        }
    }

    for extra in [
        "/tmp",
        "/tmp/snap.discord",
        "/tmp/snap-private-tmp/snap.discord/tmp",
    ] {
        let p = PathBuf::from(extra);
        if !dirs.contains(&p) {
            dirs.push(p);
        }
    }

    let mut sockets = Vec::new();
    for d in dirs {
        for i in 0..10 {
            sockets.push(d.join(format!("discord-ipc-{}", i)));
        }
    }
    sockets
}

pub fn write_ipc_frame<W: Write>(stream: &mut W, opcode: u32, payload: &str) -> std::io::Result<()> {
    let bytes = payload.as_bytes();
    let mut header = [0u8; 8];
    header[0..4].copy_from_slice(&opcode.to_le_bytes());
    header[4..8].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
    stream.write_all(&header)?;
    stream.write_all(bytes)?;
    stream.flush()
}

pub fn read_ipc_frame<R: Read>(stream: &mut R) -> std::io::Result<(u32, String)> {
    let mut header = [0u8; 8];
    stream.read_exact(&mut header)?;
    let opcode = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
    let len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
    if len > 256 * 1024 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Discord IPC frame too large",
        ));
    }
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf)?;
    Ok((opcode, String::from_utf8_lossy(&buf).into_owned()))
}

fn is_ipc_error_frame(body: &str) -> bool {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(body) {
        val["evt"].as_str() == Some("ERROR")
    } else {
        body.contains("\"evt\":\"ERROR\"")
    }
}

#[cfg(unix)]
pub fn connect_unix_path_and_handshake(
    path: &std::path::Path,
    client_id: &str,
) -> Option<std::os::unix::net::UnixStream> {
    let mut stream = std::os::unix::net::UnixStream::connect(path).ok()?;
    let _ = stream.set_read_timeout(Some(Duration::from_millis(1500)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(1500)));
    let hs = json!({
        "v": 1,
        "client_id": client_id,
    })
    .to_string();
    write_ipc_frame(&mut stream, 0, &hs).ok()?;
    let (op, body) = read_ipc_frame(&mut stream).ok()?;
    if op == 1 && !is_ipc_error_frame(&body) {
        Some(stream)
    } else {
        None
    }
}

fn open_discord_ipc(custom_client_id: Option<&str>) -> Option<(Box<dyn IpcStream>, String)> {
    let ids_to_try: Vec<&str> = match custom_client_id {
        Some(cid) if !cid.is_empty() && cid != DEFAULT_DISCORD_APP_ID => {
            vec![cid, DEFAULT_DISCORD_APP_ID]
        }
        _ => vec![DEFAULT_DISCORD_APP_ID],
    };

    #[cfg(unix)]
    {
        let candidates = candidate_unix_ipc_paths();
        for cid in ids_to_try {
            for sock_path in &candidates {
                if !sock_path.exists() {
                    continue;
                }
                if let Some(s) = connect_unix_path_and_handshake(sock_path, cid) {
                    return Some((Box::new(s), cid.to_string()));
                }
            }
        }
    }

    #[cfg(windows)]
    {
        for cid in ids_to_try {
            for i in 0..10 {
                let pipe_path = format!(r"\\.\pipe\discord-ipc-{}", i);
                if let Ok(mut f) = std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&pipe_path)
                {
                    let hs = json!({ "v": 1, "client_id": cid }).to_string();
                    if write_ipc_frame(&mut f, 0, &hs).is_ok() {
                        if let Ok((op, body)) = read_ipc_frame(&mut f) {
                            if op == 1 && !is_ipc_error_frame(&body) {
                                return Some((Box::new(f), cid.to_string()));
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

pub fn build_set_activity_payload(
    act: Option<&DiscordActivity>,
    nonce: u64,
    include_assets: bool,
) -> String {
    let pid = std::process::id();
    match act {
        Some(a) => {
            let mut activity = serde_json::Map::new();
            activity.insert("type".to_string(), json!(0));
            activity.insert("details".to_string(), json!(a.details));
            activity.insert("state".to_string(), json!(a.state));
            activity.insert("timestamps".to_string(), json!({ "start": a.start_timestamp }));
            if include_assets {
                activity.insert(
                    "assets".to_string(),
                    json!({
                        "large_image": "grass",
                        "large_text": "AxyLauncher"
                    }),
                );
            }
            json!({
                "cmd": "SET_ACTIVITY",
                "args": {
                    "pid": pid,
                    "activity": serde_json::Value::Object(activity)
                },
                "nonce": nonce.to_string(),
            })
            .to_string()
        }
        None => json!({
            "cmd": "SET_ACTIVITY",
            "args": {
                "pid": pid,
                "activity": null
            },
            "nonce": nonce.to_string(),
        })
        .to_string(),
    }
}

fn rpc_worker_loop(
    rx: mpsc::Receiver<RpcMsg>,
    on_status: Arc<dyn Fn(bool, String) + Send + Sync>,
) {
    let mut conn: Option<(Box<dyn IpcStream>, String)> = None;
    let mut desired: Option<DiscordActivity> = None;
    let mut nonce: u64 = 1;
    let mut last_connected: Option<bool> = None;

    let report_status = |connected: bool, msg: String, last: &mut Option<bool>| {
        if *last != Some(connected) {
            *last = Some(connected);
            on_status(connected, msg);
        }
    };

    loop {
        match rx.recv_timeout(Duration::from_secs(4)) {
            Ok(RpcMsg::Shutdown) => {
                if let Some((mut stream, _)) = conn.take() {
                    stream.set_nonblocking_mode(false);
                    let _ = write_ipc_frame(
                        &mut stream,
                        1,
                        &build_set_activity_payload(None, nonce, false),
                    );
                    let _ = write_ipc_frame(&mut stream, 2, "{}");
                }
                break;
            }
            Ok(RpcMsg::Clear) => {
                desired = None;
                if let Some((mut stream, _)) = conn.take() {
                    stream.set_nonblocking_mode(false);
                    let _ = write_ipc_frame(
                        &mut stream,
                        1,
                        &build_set_activity_payload(None, nonce, false),
                    );
                    nonce += 1;
                    let _ = write_ipc_frame(&mut stream, 2, "{}");
                }
                report_status(false, "[Discord RPC] Отключён в настройках.".to_string(), &mut last_connected);
            }
            Ok(RpcMsg::Set(act)) => {
                // Coalesce queued updates to the latest one
                let mut current = Some(act);
                while let Ok(next_msg) = rx.try_recv() {
                    match next_msg {
                        RpcMsg::Set(a) => current = Some(a),
                        RpcMsg::Clear => current = None,
                        RpcMsg::Shutdown => return,
                    }
                }
                match current {
                    Some(latest) => {
                        let wanted_cid = latest
                            .custom_client_id
                            .as_deref()
                            .unwrap_or(DEFAULT_DISCORD_APP_ID);
                        if let Some((_, ref active_cid)) = conn {
                            if active_cid != wanted_cid {
                                conn = None;
                            }
                        }
                        desired = Some(latest);
                        let (ok, status_msg) = send_or_reconnect(&mut conn, desired.as_ref(), &mut nonce);
                        report_status(ok, status_msg, &mut last_connected);
                    }
                    None => {
                        desired = None;
                        if let Some((mut stream, _)) = conn.take() {
                            stream.set_nonblocking_mode(false);
                            let _ = write_ipc_frame(
                                &mut stream,
                                1,
                                &build_set_activity_payload(None, nonce, false),
                            );
                            nonce += 1;
                            let _ = write_ipc_frame(&mut stream, 2, "{}");
                        }
                        report_status(false, "[Discord RPC] Отключён.".to_string(), &mut last_connected);
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if desired.is_some() {
                    if let Some((ref mut stream, _)) = conn {
                        // Drain incoming frames & respond to PINGs so socket stays healthy
                        stream.set_nonblocking_mode(true);
                        let mut broken = false;
                        while let Ok((op, payload)) = read_ipc_frame(stream) {
                            if op == 3 {
                                stream.set_nonblocking_mode(false);
                                if write_ipc_frame(stream, 4, &payload).is_err() {
                                    broken = true;
                                    break;
                                }
                                stream.set_nonblocking_mode(true);
                            } else if op == 2 {
                                broken = true;
                                break;
                            }
                        }
                        if broken {
                            conn = None;
                        }
                    }
                    if conn.is_none() {
                        let (ok, status_msg) = send_or_reconnect(&mut conn, desired.as_ref(), &mut nonce);
                        report_status(ok, status_msg, &mut last_connected);
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn send_or_reconnect(
    conn: &mut Option<(Box<dyn IpcStream>, String)>,
    desired: Option<&DiscordActivity>,
    nonce: &mut u64,
) -> (bool, String) {
    let Some(act) = desired else {
        return (false, String::new());
    };
    if conn.is_none() {
        *conn = open_discord_ipc(act.custom_client_id.as_deref());
    }
    let Some((ref mut stream, ref active_cid)) = conn else {
        return (
            false,
            "[Discord RPC] Не найден запущенный клиент Discord / Vesktop (discord-ipc-0..9).".to_string(),
        );
    };

    stream.set_nonblocking_mode(false);
    let use_assets = active_cid == DEFAULT_DISCORD_APP_ID;
    let payload = build_set_activity_payload(Some(act), *nonce, use_assets);
    *nonce += 1;
    if write_ipc_frame(stream, 1, &payload).is_err() {
        *conn = None;
        return (false, "[Discord RPC] Потеряно соединение с IPC-сокетом.".to_string());
    }

    match read_ipc_frame(stream) {
        Ok((_op, resp)) if is_ipc_error_frame(&resp) => {
            if use_assets {
                // Retry once without `assets` in case Discord rejects asset key
                let fallback_payload = build_set_activity_payload(Some(act), *nonce, false);
                *nonce += 1;
                if write_ipc_frame(stream, 1, &fallback_payload).is_ok() {
                    if let Ok((_op2, resp2)) = read_ipc_frame(stream) {
                        if !is_ipc_error_frame(&resp2) {
                            return (
                                true,
                                format!("[Discord RPC] Подключено (App ID: {}).", active_cid),
                            );
                        }
                    }
                }
            }
            *conn = None;
            (false, format!("[Discord RPC] Discord отклонил статус: {}", resp))
        }
        Ok(_) => (
            true,
            format!("[Discord RPC] Подключено (App ID: {}).", active_cid),
        ),
        Err(_) => {
            *conn = None;
            (false, "[Discord RPC] Нет ответа от Discord IPC.".to_string())
        }
    }
}
