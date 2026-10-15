//! Owner-configured read-only MCP connections. No server can widen its allowlist.
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Connection {
    pub id: String,
    pub url: String,
    pub token_file: Option<String>,
    pub tools: BTreeMap<String, Tool>,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tool {
    pub description: String,
    pub read_only: bool,
    pub schema: Value,
}
#[derive(Clone, Default)]
pub struct Registry {
    pub entries: Vec<Connection>,
    pub fingerprint: String,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Call {
    pub connection: String,
    pub tool: String,
    pub arguments: Value,
}
impl Registry {
    pub fn load(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self::default());
        }
        use std::os::unix::fs::MetadataExt;
        let meta = std::fs::symlink_metadata(path).map_err(|_| "connection config unavailable")?;
        if !meta.is_file() || meta.mode() & 0o077 != 0 || meta.uid() != unsafe { libc::geteuid() } {
            return Err("connections.json must be owned by this user and mode 0600".into());
        }
        if meta.len() > 65536 {
            return Err("connection config exceeds 64 KiB".into());
        }
        let body =
            crate::strict_json::parse(&std::fs::read(path).map_err(|_| "config unreadable")?)
                .map_err(|_| "invalid connection JSON")?;
        let entries: Vec<Connection> =
            serde_json::from_value(body.clone()).map_err(|_| "invalid connection fields")?;
        let mut ids = std::collections::HashSet::new();
        if entries.len() > 16 {
            return Err("maximum 16 configured connections".into());
        }
        for c in &entries {
            if c.id.is_empty()
                || !c
                    .id
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
                || !ids.insert(&c.id)
            {
                return Err("invalid or duplicate connection ID".into());
            }
            let url = reqwest::Url::parse(&c.url).map_err(|_| "invalid connection URL")?;
            let local = matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"));
            if !(url.scheme() == "https" || (url.scheme() == "http" && local))
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err(
                    "connections require HTTPS or loopback HTTP; credentials must not be in URLs"
                        .into(),
                );
            }
            for t in c.tools.values() {
                if !t.read_only {
                    return Err(
                        "this release supports owner-allowlisted read-only tools only".into(),
                    );
                }
                jsonschema::validator_for(&t.schema)
                    .map_err(|_| "invalid or externally referenced tool schema")?;
            }
        }
        Ok(Self {
            entries,
            fingerprint: crate::jcs::canonical_digest(&body)
                .map_err(|_| "invalid config digest")?,
        })
    }
    pub fn list(&self) -> Value {
        json!(self.entries.iter().map(|c|json!({"id":c.id,"endpoint":c.url,"status":"configured—not yet checked","tools":c.tools.iter().map(|(name,t)|json!({"name":name,"description":t.description,"schema":t.schema})).collect::<Vec<_>>(),"read_only":true})).collect::<Vec<_>>())
    }
    pub fn validate(&self, args: &Value) -> Result<Call, String> {
        let call: Call = serde_json::from_value(args.clone())
            .map_err(|_| "invalid connection.call arguments")?;
        let c = self
            .entries
            .iter()
            .find(|c| c.id == call.connection)
            .ok_or("connection is not configured")?;
        let t = c
            .tools
            .get(&call.tool)
            .ok_or("tool is not owner-allowlisted")?;
        if !jsonschema::validator_for(&t.schema)
            .map_err(|_| "invalid configured schema")?
            .is_valid(&call.arguments)
        {
            return Err("arguments do not match the owner-approved tool schema".into());
        }
        if args.to_string().len() > 16384 {
            return Err("connection arguments exceed 16 KiB".into());
        }
        Ok(call)
    }
    pub fn description(&self, args: &Value) -> Result<String, String> {
        let call = self.validate(args)?;
        let c = self
            .entries
            .iter()
            .find(|c| c.id == call.connection)
            .unwrap();
        Ok(format!("Send exact arguments to {} ({}) using owner-allowlisted read-only tool {}; return its result to the configured model",c.id,c.url,call.tool))
    }
    pub async fn check(&self, client: &reqwest::Client, id: &str) -> Result<Value, String> {
        let c = self
            .entries
            .iter()
            .find(|c| c.id == id)
            .ok_or("unknown connection")?;
        let mut session = Mcp::start(client, c).await?;
        let result = session.rpc("tools/list", json!({})).await;
        session.close().await;
        let result = result?;
        let tools = result["tools"]
            .as_array()
            .ok_or("server returned no tools")?;
        let names: Vec<_> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        let missing: Vec<_> = c
            .tools
            .keys()
            .filter(|name| !names.contains(&name.as_str()))
            .collect();
        Ok(
            json!({"id":id,"reachable":true,"missing_allowlisted_tools":missing,"available_tools":names,"note":"schema and read-only declarations require owner review; server annotations do not authorize execution"}),
        )
    }
    pub async fn execute(&self, client: &reqwest::Client, args: &Value) -> Result<Value, String> {
        let call = self.validate(args)?;
        let c = self
            .entries
            .iter()
            .find(|c| c.id == call.connection)
            .unwrap();
        let mut session = Mcp::start(client, c).await?;
        let result = session
            .rpc(
                "tools/call",
                json!({"name":call.tool,"arguments":call.arguments}),
            )
            .await;
        session.close().await;
        result
    }
}
struct Mcp<'a> {
    client: &'a reqwest::Client,
    connection: &'a Connection,
    token: Option<String>,
    session: Option<String>,
    version: String,
    next: u64,
}
impl<'a> Mcp<'a> {
    async fn start(client: &'a reqwest::Client, c: &'a Connection) -> Result<Self, String> {
        let token = if let Some(file) = &c.token_file {
            Some(
                crate::private_token(Path::new(file))
                    .map_err(|_| "connection token file unavailable or insecure")?,
            )
        } else {
            None
        };
        let mut m = Self {
            client,
            connection: c,
            token,
            session: None,
            version: "2025-06-18".into(),
            next: 1,
        };
        let init=m.rpc("initialize",json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"jarvis","version":"0.1.0"}})).await?;
        let version = init["protocolVersion"]
            .as_str()
            .ok_or("missing MCP protocol version")?;
        if !matches!(version, "2025-06-18" | "2025-03-26") {
            return Err("unsupported MCP protocol version".into());
        }
        m.version = version.into();
        let response = m
            .post(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
            .await?;
        if response.status() != reqwest::StatusCode::ACCEPTED {
            return Err("MCP server rejected initialization notification".into());
        }
        Ok(m)
    }
    fn request(&self, method: reqwest::Method) -> reqwest::RequestBuilder {
        let mut r = self
            .client
            .request(method, &self.connection.url)
            .timeout(std::time::Duration::from_secs(15))
            .header("accept", "application/json, text/event-stream")
            .header("MCP-Protocol-Version", &self.version);
        if let Some(token) = &self.token {
            r = r.bearer_auth(token);
        }
        if let Some(session) = &self.session {
            r = r.header("Mcp-Session-Id", session);
        }
        r
    }
    async fn post(&self, body: Value) -> Result<reqwest::Response, String> {
        self.request(reqwest::Method::POST)
            .json(&body)
            .send()
            .await
            .map_err(|_| "MCP connection failed or timed out".into())
    }
    async fn rpc(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next;
        self.next += 1;
        let response = self
            .post(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
            .await?;
        if !response.status().is_success() {
            return Err(format!("MCP request failed ({})", response.status()));
        }
        if let Some(s) = response.headers().get("Mcp-Session-Id") {
            self.session = Some(s.to_str().map_err(|_| "invalid MCP session")?.into());
        }
        let sse = response
            .headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("")
            .starts_with("text/event-stream");
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::new();
        let mut total = 0;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| "MCP stream interrupted")?;
            total += chunk.len();
            if total > 65536 {
                return Err("MCP response exceeds 64 KiB".into());
            }
            bytes.extend_from_slice(&chunk);
            if sse {
                while let Some((end, width)) = boundary(&bytes) {
                    let frame: Vec<u8> = bytes.drain(..end + width).collect();
                    let frame = std::str::from_utf8(&frame).map_err(|_| "invalid MCP UTF-8")?;
                    let data = frame
                        .lines()
                        .filter_map(|l| l.strip_prefix("data:"))
                        .map(str::trim)
                        .collect::<Vec<_>>()
                        .join("\n");
                    if data.is_empty() {
                        continue;
                    }
                    let v = crate::strict_json::parse(data.as_bytes())
                        .map_err(|_| "invalid MCP JSON")?;
                    if v["id"] == id {
                        return Self::result(v);
                    }
                }
            }
        }
        if sse {
            return Err("MCP stream ended without a matching response".into());
        }
        let v = crate::strict_json::parse(&bytes).map_err(|_| "invalid MCP response")?;
        if v["id"] != id {
            return Err("MCP response ID mismatch".into());
        }
        Self::result(v)
    }
    fn result(v: Value) -> Result<Value, String> {
        if v["jsonrpc"] != "2.0" || v.get("error").is_some() {
            return Err("MCP server reported an error".into());
        }
        v.get("result").cloned().ok_or("MCP result missing".into())
    }
    async fn close(&self) {
        if self.session.is_some() {
            let _ = self
                .request(reqwest::Method::DELETE)
                .timeout(std::time::Duration::from_secs(2))
                .send()
                .await;
        }
    }
}

fn boundary(bytes: &[u8]) -> Option<(usize, usize)> {
    let lf = bytes.windows(2).position(|w| w == b"\n\n").map(|p| (p, 2));
    let crlf = bytes
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|p| (p, 4));
    match (lf, crlf) {
        (Some(a), Some(b)) => Some(if a.0 < b.0 { a } else { b }),
        (Some(a), None) | (None, Some(a)) => Some(a),
        _ => None,
    }
}
