//! Interactive owner-authenticated client. Credentials never enter URLs.
use rand::RngCore;
use serde_json::{json, Value};
use std::{
    env,
    fs::{self, OpenOptions},
    io::{self, Write},
    os::unix::fs::OpenOptionsExt,
    path::PathBuf,
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn config() -> Result<PathBuf> {
    Ok(PathBuf::from(env::var("JARVIS_CONFIG_DIR").unwrap_or(
        format!("{}/.config/jarvis", env::var("HOME")?),
    )))
}
fn random() -> [u8; 32] {
    let mut bytes = [0; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes
}
fn secret(path: PathBuf, bytes: &[u8]) -> Result<()> {
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
    {
        Ok(mut f) => {
            f.write_all(bytes)?;
            f.sync_all()?;
        }
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.into()),
    }
    Ok(())
}
fn init() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let dir = config()?;
    fs::create_dir_all(&dir)?;
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
    secret(dir.join("key"), &random())?;
    for name in ["token", "gateway-token"] {
        let token: String = random().iter().map(|b| format!("{b:02x}")).collect();
        secret(dir.join(name), token.as_bytes())?;
    }
    let export = PathBuf::from(env::var("HOME")?).join(".local/share/jarvis/export");
    fs::create_dir_all(&export)?;
    println!(
        "Initialized {} and {}. Existing secrets preserved.\nSee docs/RUNNING.md to start JARVIS.",
        dir.display(),
        export.display()
    );
    Ok(())
}
struct Client {
    http: reqwest::blocking::Client,
    url: String,
    token: String,
}
impl Client {
    fn new() -> Result<Self> {
        let url = env::var("JARVIS_CORE_URL").unwrap_or("http://127.0.0.1:8790".into());
        let parsed = reqwest::Url::parse(&url)?;
        if parsed.scheme() != "http"
            || parsed.host_str() != Some("127.0.0.1")
            || parsed.path() != "/"
        {
            return Err("core URL must be a loopback HTTP origin at 127.0.0.1".into());
        }
        let token = fs::read_to_string(config()?.join("token"))?;
        let http = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(40))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self {
            http,
            url: url.trim_end_matches('/').into(),
            token: token.trim().into(),
        })
    }
    fn call(&self, body: Value) -> Result<Value> {
        let r = self
            .http
            .post(format!("{}/v1/command", self.url))
            .bearer_auth(&self.token)
            .json(&body)
            .send()?;
        let status = r.status();
        let v: Value = r.json()?;
        if !status.is_success() {
            return Err(v["error"]
                .as_str()
                .unwrap_or("request failed")
                .to_string()
                .into());
        }
        Ok(v)
    }
}
fn line(prompt: &str) -> Result<String> {
    print!("{prompt}");
    io::stdout().flush()?;
    let mut s = String::new();
    if io::stdin().read_line(&mut s)? == 0 {
        return Ok("/quit".into());
    }
    Ok(s.trim().into())
}
fn pretty(v: Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&v)?);
    Ok(())
}
fn turn(c: &Client, mut v: Value) -> Result<()> {
    loop {
        if let Some(s) = v["response"].as_str() {
            println!("\nJARVIS: {s}\n");
        }
        if let Some(s) = v["error"].as_str() {
            println!("JARVIS: {s}");
        }
        if v["response"].is_null() {
            println!(
                "Task {} — {}",
                v["task_id"].as_str().unwrap_or(""),
                v["state"].as_str().unwrap_or("")
            );
        }
        let p = &v["proposal"];
        if p.is_null() {
            break;
        }
        println!("Approval: {} v1\nExact arguments: {}\n{}\nOne use; file contents at dispatch; 64 KiB limit.",p["tool"].as_str().unwrap_or(""),p["arguments"],p["disclosure"].as_str().unwrap_or(""));
        if p["expired"] == true && p["reapproval_required"] != true {
            println!("Expired. Cancel the task and submit a new request.");
            break;
        }
        match line("Type approve, deny, or later: ")?.as_str() {
            "approve" => {
                v=c.call(json!({"type":"approval.grant","task_id":v["task_id"],"proposal_id":p["proposal_id"],"digest":p["digest"]}))?;
            }
            "deny" => {
                pretty(c.call(json!({"type":"approval.deny","task_id":v["task_id"],"proposal_id":p["proposal_id"]}))?)?;
                break;
            }
            _ => {
                println!("Left pending. /approve TASK_ID or /cancel TASK_ID when ready.");
                break;
            }
        }
    }
    Ok(())
}
fn chat(c: &Client, sid: Option<String>) -> Result<()> {
    let mut sid = match sid {
        Some(s) => s,
        None => c.call(json!({"type":"session.open"}))?["session_id"]
            .as_str()
            .ok_or("missing session")?
            .into(),
    };
    let status = c.call(json!({"type":"system.status"}))?;
    println!(
        "JARVIS • {} / {} • session {}",
        status["provider"].as_str().unwrap_or(""),
        status["model"].as_str().unwrap_or(""),
        sid
    );
    if status["mode"] == "demo" {
        println!("DEMO MODE: deterministic responses; no real model connected.");
    }
    println!("/help for commands. /quit exits and preserves pending work.");
    loop {
        let input = line("You: ")?;
        if input == "/quit" {
            break;
        }
        if input.is_empty() {
            continue;
        }
        let result: Result<()> = (|| {
            match input.as_str() {
                "/help" => {
                    println!("/status /tasks /connections /check CONNECTION_ID /memory /remember KEY=VALUE /forget MEMORY_ID\n/approve TASK_ID /review TASK_ID /resume TASK_ID /cancel TASK_ID /new /quit\nResume this session: jarvis-cli chat {sid}");
                    return Ok(());
                }
                "/connections" => return pretty(c.call(json!({"type":"connection.list"}))?),
                "/status" => return pretty(c.call(json!({"type":"system.status"}))?),
                "/tasks" => return pretty(c.call(json!({"type":"task.list"}))?),
                "/memory" => return pretty(c.call(json!({"type":"memory.list"}))?),
                "/new" => {
                    sid = c.call(json!({"type":"session.open"}))?["session_id"]
                        .as_str()
                        .ok_or("missing session")?
                        .into();
                    println!("Session {sid}");
                    return Ok(());
                }
                _ => {}
            }
            if let Some(s) = input.strip_prefix("/remember ") {
                let (key, value) = s.split_once('=').ok_or("use /remember KEY=VALUE")?;
                let p =
                    c.call(json!({"type":"memory.propose","key":key.trim(),"value":value.trim()}))?;
                println!("Save this exact fact for 30 days? {}", p["content"]);
                if line("Type confirm to save: ")? == "confirm" {
                    c.call(json!({"type":"memory.confirm","memory_id":p["memory_id"],"digest":p["digest"]}))?;
                    println!("Saved.");
                } else {
                    println!("Not confirmed; candidate expires in 24 hours.");
                }
                return Ok(());
            }
            for (prefix, kind) in [
                ("/check ", "connection.check"),
                ("/review ", "task.repropose"),
                ("/forget ", "memory.delete"),
                ("/cancel ", "task.cancel"),
                ("/resume ", "task.resume"),
                ("/approve ", "task.get"),
            ] {
                if let Some(id) = input.strip_prefix(prefix) {
                    let field = if kind == "connection.check" {
                        "connection_id"
                    } else if kind == "memory.delete" {
                        "memory_id"
                    } else {
                        "task_id"
                    };
                    let mut cmd = json!({"type":kind});
                    cmd[field] = json!(id.trim());
                    let v = c.call(cmd)?;
                    if prefix == "/forget " || prefix == "/check " {
                        return pretty(v);
                    }
                    return turn(c, v);
                }
            }
            if input.starts_with('/') {
                return Err("unknown command; use /help".into());
            }
            turn(c,c.call(json!({"type":"input.send","session_id":sid,"request_id":uuid::Uuid::new_v4().to_string(),"text":input}))?)
        })();
        if let Err(e) = result {
            eprintln!("{e}");
        }
    }
    Ok(())
}
fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("init") => init(),
        Some("chat") | None => chat(&Client::new()?, args.next()),
        Some("pair") => pretty(Client::new()?.call(json!({"type":"pair.create"}))?),
        Some("status") => pretty(Client::new()?.call(json!({"type":"system.status"}))?),
        Some("command") => pretty(Client::new()?.call(serde_json::from_str(
            &args.next().ok_or("command requires a JSON argument")?,
        )?)?),
        _ => Err("usage: jarvis-cli [init | chat [SESSION_ID] | status | command JSON]".into()),
    }
}
