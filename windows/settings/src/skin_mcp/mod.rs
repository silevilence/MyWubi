//! MCP requests run on the editor thread, against the same draft the user sees.
mod author;
pub mod control;
mod docs;
mod preview;
#[cfg(test)]
mod tests;
use crate::skin_editor::SkinEditor;
use eframe::egui;
use serde_json::{json, Value};
use std::{
    io::Read,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::Duration,
};
use tiny_http::{Header, Method, Request, Response, Server};
use uuid::Uuid;

const VERSION: &str = "2025-11-25";
struct Call {
    rpc: Value,
    reply: mpsc::Sender<Value>,
}

pub struct Endpoint {
    pub url: String,
    calls: mpsc::Receiver<Call>,
    stopped: Arc<AtomicBool>,
    owner: Option<String>,
    last_write: std::time::Instant,
    pub operation: String,
}
impl Drop for Endpoint {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
    }
}

impl Endpoint {
    pub fn start(ctx: egui::Context) -> Result<Self, String> {
        let server = Server::http(("127.0.0.1", 0)).map_err(|e| e.to_string())?;
        let authority = server.server_addr().to_string();
        let path = format!("/mcp/{}", Uuid::new_v4());
        let url = format!("http://{authority}{path}");
        let (send, calls) = mpsc::channel();
        let stopped = Arc::new(AtomicBool::new(false));
        let shutdown = stopped.clone();
        std::thread::Builder::new()
            .name("skin-mcp".into())
            .spawn(move || {
                while !shutdown.load(Ordering::Acquire) {
                    match server.recv_timeout(Duration::from_millis(100)) {
                        Ok(Some(req)) => serve(req, &authority, &path, &send, &ctx),
                        Ok(None) => continue,
                        Err(_) => break,
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            url,
            calls,
            stopped,
            owner: None,
            last_write: std::time::Instant::now(),
            operation: String::new(),
        })
    }
    pub fn update(&mut self, editor: &mut SkinEditor) {
        if self.owner.is_some() && self.last_write.elapsed() >= Duration::from_secs(300) {
            self.owner = None;
            self.operation.clear();
        }
        if let Ok(call) = self.calls.try_recv() {
            let result = self.rpc(&call.rpc, editor);
            let _ = call.reply.send(result);
        }
    }
    pub fn active(&self) -> bool {
        self.owner.is_some()
    }
    fn rpc(&mut self, rpc: &Value, editor: &mut SkinEditor) -> Value {
        let id = &rpc["id"];
        let result = match rpc["method"].as_str() {
            Some("initialize") => {
                json!({"protocolVersion":VERSION,"capabilities":{"tools":{},"resources":{}},
                "serverInfo":{"name":"mywubi-skin-editor","version":env!("CARGO_PKG_VERSION")},
                "instructions":"Use acquire before editing. Pass its lease to editing tools. Release when finished."})
            }
            Some("resources/list") => json!({"resources":docs::list()}),
            Some("resources/read") => {
                match docs::read(rpc["params"]["uri"].as_str().unwrap_or_default()) {
                    Ok(c) => json!({"contents":[c]}),
                    Err(e) => return error(id, -32002, &e),
                }
            }
            Some("ping") => json!({}),
            Some("tools/list") => json!({"tools":self.tools()}),
            Some("tools/call") => {
                let name = rpc["params"]["name"].as_str().unwrap_or_default();
                match self.call(name, &rpc["params"]["arguments"], editor) {
                    Ok(content) => json!({"content":content}),
                    Err(message) => {
                        json!({"isError":true,"content":[{"type":"text","text":message}]})
                    }
                }
            }
            _ => return error(id, -32601, "Method not found"),
        };
        json!({"jsonrpc":"2.0","id":id,"result":result})
    }
    fn tools(&self) -> Vec<Value> {
        let mut tools = vec![
            tool(
                "lookup_skin_docs",
                "Search bundled authoring documentation in Chinese or English",
                json!({"query":{"type":"string"}}),
                &["query"],
            ),
            tool(
                "status",
                "Current draft and authoring state",
                json!({}),
                &[],
            ),
            tool(
                "acquire",
                "Acquire exclusive editing control for up to 300 idle seconds",
                json!({}),
                &[],
            ),
            tool(
                "release",
                "Return editing control",
                json!({"lease":{"type":"string"}}),
                &["lease"],
            ),
        ];
        tools.extend(author::tools());
        tools.push(preview::definition());
        tools
    }
    fn call(&mut self, name: &str, args: &Value, editor: &mut SkinEditor) -> Result<Value, String> {
        let result = match name {
            "lookup_skin_docs" => docs::lookup(args["query"].as_str().unwrap_or_default())?,
            "status" => {
                json!({"draft":editor.title(),"hasDraft":editor.has_draft(),"controlled":self.active(),"operation":self.operation})
            }
            "acquire" if self.owner.is_none() => {
                let lease = Uuid::new_v4().to_string();
                self.last_write = std::time::Instant::now();
                self.owner = Some(lease.clone());
                self.operation = "已接管草稿".into();
                json!({"lease":lease})
            }
            "acquire" => return Err("当前草稿已有 Agent 接管".into()),
            "release" => {
                self.check_owner(args)?;
                self.owner = None;
                self.operation.clear();
                json!({"released":true})
            }
            "preview" => return preview::run(args, editor),
            "inspect" | "validate" => author::read(name, args, editor)?,
            _ => {
                self.check_owner(args)?;
                self.operation = format!(
                    "{} {} {} {}",
                    name,
                    args["platform"].as_str().unwrap_or(""),
                    args["component"].as_str().unwrap_or(""),
                    args["variant"].as_str().unwrap_or("")
                )
                .trim()
                .into();
                author::write(name, args, editor)?
            }
        };
        Ok(text_result(result))
    }
    fn check_owner(&mut self, args: &Value) -> Result<(), String> {
        if self.owner.is_some() && args["lease"].as_str() == self.owner.as_deref() {
            self.last_write = std::time::Instant::now();
            Ok(())
        } else {
            Err("接管凭据无效，请先 acquire".into())
        }
    }
}

fn text_result(value: Value) -> Value {
    json!([{"type":"text","text":value.to_string()}])
}
fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false}})
}
fn error(id: &Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}
fn respond(req: Request, status: u16, value: Option<Value>) {
    let mut reply = Response::from_string(value.map(|v| v.to_string()).unwrap_or_default())
        .with_status_code(status);
    if let Ok(header) = Header::from_bytes("Content-Type", "application/json") {
        reply.add_header(header);
    }
    let _ = req.respond(reply);
}
fn serve(
    mut req: Request,
    authority: &str,
    path: &str,
    send: &mpsc::Sender<Call>,
    ctx: &egui::Context,
) {
    let header = |key: &'static str| {
        req.headers()
            .iter()
            .find(|h| h.field.equiv(key))
            .map(|h| h.value.as_str())
    };
    if header("Host") != Some(authority)
        || header("Origin").is_some_and(|o| o != format!("http://{authority}"))
    {
        return respond(req, 403, None);
    }
    if req.url() != path {
        return respond(req, 404, None);
    }
    if req.method() != &Method::Post {
        return respond(req, 405, None);
    }
    const MAX: u64 = 90 * 1024 * 1024;
    let mut body = Vec::new();
    if req.body_length().is_some_and(|n| n as u64 > MAX)
        || req
            .as_reader()
            .take(MAX + 1)
            .read_to_end(&mut body)
            .is_err()
        || body.len() as u64 > MAX
    {
        return respond(req, 413, None);
    }
    let Ok(rpc) = serde_json::from_slice::<Value>(&body) else {
        return respond(req, 400, Some(error(&Value::Null, -32700, "Parse error")));
    };
    if rpc["jsonrpc"] != "2.0" || !rpc["method"].is_string() {
        return respond(
            req,
            400,
            Some(error(&Value::Null, -32600, "Invalid request")),
        );
    }
    if rpc.get("id").is_none() {
        return respond(req, 202, None);
    }
    let (reply, result) = mpsc::channel();
    if send.send(Call { rpc, reply }).is_err() {
        return respond(req, 503, None);
    }
    ctx.request_repaint();
    match result.recv() {
        Ok(value) => respond(req, 200, Some(value)),
        Err(_) => respond(req, 503, None),
    }
}
