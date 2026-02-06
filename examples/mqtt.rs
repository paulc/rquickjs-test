use anyhow::anyhow;
use argh::FromArgs;
use rquickjs::function::Func;
use rquickjs::{
    async_with, class::Trace, ArrayBuffer, AsyncContext, AsyncRuntime, CatchResultExt, Class, Ctx,
    Exception, JsLifetime, Module, Value,
};
use serde::{Deserialize, Serialize};

use std::io::{Read, Write};

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::time::{timeout, Duration};

// We need to define a local QoS enum which is serialisable
#[derive(Trace, JsLifetime, Debug, Clone, Serialize, Deserialize)]
#[rquickjs::class]
pub enum QoS {
    AtMostOnce,
    AtLeastOnce,
    ExactlyOnce,
}

impl std::fmt::Display for QoS {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QoS::AtMostOnce => write!(f, "AtMostOnce"),
            QoS::AtLeastOnce => write!(f, "AtLeastOnce"),
            QoS::ExactlyOnce => write!(f, "ExactlyOnce"),
        }
    }
}

impl TryFrom<&str> for QoS {
    type Error = ();
    fn try_from(s: &str) -> Result<Self, ()> {
        match s.to_lowercase().as_str() {
            "qos0" | "atmostonce" => Ok(QoS::AtMostOnce),
            "qos1" | "atleastonce" => Ok(QoS::AtLeastOnce),
            "qos2" | "exactlyonce" => Ok(QoS::ExactlyOnce),
            _ => Err(()),
        }
    }
}

#[derive(Trace, JsLifetime, Debug, Clone, Serialize, Deserialize)]
#[rquickjs::class]
pub enum MqttCommand {
    /// Publish a message to a topic
    Publish {
        topic: String,
        payload: Vec<u8>,
        qos: QoS,
    },
    /// Subscribe to a topic
    Subscribe { topic: String, qos: QoS },
    /// Unsubscribe from a topic
    Unsubscribe { topic: String },
    /// Disconnect from the MQTT broker
    Disconnect,
}

#[rquickjs::methods]
impl MqttCommand {
    #[qjs(get, rename = "type")]
    pub fn get_type(&self) -> String {
        match self {
            MqttCommand::Publish { .. } => "Publish".to_string(),
            MqttCommand::Subscribe { .. } => "Subscribe".to_string(),
            MqttCommand::Unsubscribe { .. } => "Unsubscribe".to_string(),
            MqttCommand::Disconnect => "Disconnect".to_string(),
        }
    }

    #[qjs(get, rename = "payload")]
    pub fn get_payload<'js>(&self, ctx: Ctx<'js>) -> rquickjs::Result<Value<'js>> {
        match self {
            MqttCommand::Publish { payload, .. } => {
                Ok(ArrayBuffer::new_copy(ctx.clone(), payload)?
                    .as_value()
                    .clone())
            }
            _ => Ok(rquickjs::Undefined {}.into_value(ctx.clone())),
        }
    }

    #[qjs(get, rename = "payload_utf8")]
    pub fn get_payload_utf8<'js>(&self, ctx: Ctx<'js>) -> rquickjs::Result<Value<'js>> {
        match self {
            MqttCommand::Publish { payload, .. } => {
                Ok(ArrayBuffer::new_copy(ctx.clone(), payload)?
                    .as_value()
                    .clone())
            }
            _ => Ok(rquickjs::Undefined {}.into_value(ctx.clone())),
        }
    }

    #[qjs(get, rename = "topic")]
    pub fn get_topic<'js>(&self, ctx: Ctx<'js>) -> rquickjs::Result<Value<'js>> {
        match self {
            MqttCommand::Publish { topic, .. }
            | MqttCommand::Subscribe { topic, .. }
            | MqttCommand::Unsubscribe { topic, .. } => {
                Ok(rquickjs::String::from_str(ctx.clone(), topic)?
                    .as_value()
                    .clone())
            }
            MqttCommand::Disconnect => Ok(rquickjs::Undefined {}.into_value(ctx.clone())),
        }
    }

    #[qjs(get, rename = "qos")]
    pub fn get_qos<'js>(&self, ctx: Ctx<'js>) -> rquickjs::Result<Value<'js>> {
        match self {
            MqttCommand::Publish { qos, .. } | MqttCommand::Subscribe { qos, .. } => {
                Ok(rquickjs::String::from_str(ctx.clone(), &qos.to_string())?
                    .as_value()
                    .clone())
            }
            _ => Ok(rquickjs::Undefined {}.into_value(ctx.clone())),
        }
    }

    pub fn debug<'js>(&self) -> rquickjs::Result<String> {
        Ok(format!("{:?}", self))
    }
}

#[rquickjs::function]
fn mqtt_message(
    ctx: Ctx<'_>,
    topic: String,
    payload: ArrayBuffer<'_>,
    qos: String,
) -> rquickjs::Result<MqttCommand> {
    Ok(MqttCommand::Publish {
        topic,
        payload: match payload.as_bytes() {
            Some(s) => s.to_vec(),
            None => Vec::new(),
        },
        qos: QoS::try_from(qos.as_str())
            .map_err(|_| Exception::throw_message(&ctx, "Invalid QoS"))?,
    })
}

#[rquickjs::function]
pub fn mqtt_utf8_message(
    ctx: Ctx<'_>,
    topic: String,
    payload: String,
    qos: String,
) -> rquickjs::Result<MqttCommand> {
    Ok(MqttCommand::Publish {
        topic,
        payload: payload.into_bytes(),
        qos: QoS::try_from(qos.as_str())
            .map_err(|_| Exception::throw_message(&ctx, "Invalid QoS"))?,
    })
}

#[rquickjs::function]
pub fn mqtt_subscribe(ctx: Ctx<'_>, topic: String, qos: String) -> rquickjs::Result<MqttCommand> {
    Ok(MqttCommand::Subscribe {
        topic,
        qos: QoS::try_from(qos.as_str())
            .map_err(|_| Exception::throw_message(&ctx, "Invalid QoS"))?,
    })
}

#[rquickjs::function]
pub fn mqtt_unsubscribe(topic: String) -> MqttCommand {
    MqttCommand::Unsubscribe { topic }
}

#[rquickjs::function]
pub fn mqtt_disconnect() -> MqttCommand {
    MqttCommand::Disconnect
}

#[derive(Trace, JsLifetime, Debug, Clone, Serialize, Deserialize)]
#[rquickjs::class]
pub enum MqttEvent {
    /// A message was received on a subscribed topic
    MessageReceived { topic: String, payload: Vec<u8> },
    /// Successfully connected to the MQTT broker
    Connected,
    /// Disconnected from the MQTT broker
    Disconnected,
    /// An error occurred
    Error(String),
}

#[rquickjs::methods]
impl MqttEvent {}

#[rquickjs::function]
fn print(s: String) {
    println!("{}", s);
}

#[rquickjs::function]
fn print_v<'js>(ctx: Ctx<'js>, v: Value<'js>) -> rquickjs::Result<()> {
    let output = ctx
        .json_stringify(v)?
        .and_then(|s| s.as_string().map(|s| s.to_string().ok()).flatten())
        .unwrap_or_else(|| "<ERR>".to_string());
    println!("{}", output);
    Ok(())
}

#[rquickjs::function]
async fn sleep(n: u64) -> rquickjs::Result<()> {
    tokio::time::sleep(Duration::from_secs(n)).await;
    Ok(())
}

#[derive(FromArgs)]
/// Async Channel
struct CliArgs {
    #[argh(option)]
    /// QJS script
    script: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: CliArgs = argh::from_env();

    let script = match args.script {
        Some(s) => Some(if s == "-" {
            let mut s = String::new();
            std::io::stdin().read_to_string(&mut s)?;
            s
        } else if s.starts_with("@") {
            std::fs::read_to_string(&s[1..])?
        } else {
            s
        }),
        None => None,
    };

    let rt = AsyncRuntime::new()?;
    let ctx = AsyncContext::full(&rt).await?;

    // let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let (resolve_tx, resolve_rx) = tokio::sync::oneshot::channel::<String>();

    async_with!(ctx => |ctx| {
        // Register functions
        ctx.globals().set("print", js_print)?;
        ctx.globals().set("print_v", js_print_v)?;
        ctx.globals().set("sleep", js_sleep)?;
        ctx.globals().set("mqtt_message", js_mqtt_message)?;
        ctx.globals().set("mqtt_utf8_message", js_mqtt_utf8_message)?;
        ctx.globals().set("mqtt_subscribe", js_mqtt_subscribe)?;
        ctx.globals().set("mqtt_unsubscribe", js_mqtt_unsubscribe)?;
        ctx.globals().set("mqtt_disconnect", js_mqtt_disconnect)?;

        // Register classes
        Class::<MqttCommand>::define(&ctx.globals()).unwrap();

        // With oneshot need to wrap tx to make sure closure is Fn vs FnOnce (send consumes tx)
        let resolve_tx = std::sync::Mutex::new(Some(resolve_tx));
        ctx.globals().set("resolve", Func::new(move |result: String| {
            if let Ok(mut guard) = resolve_tx.lock() {
                if let Some(resolve_tx) = guard.take() {
                    let _ = resolve_tx.send(result);
                }
            }
        }))?;

        /*
        // Make sure rx is Copy (Fn vs FnOnce)
        let rx = std::sync::Arc::new(std::sync::Mutex::new(rx));
        ctx.globals().set("msg_rx", Func::new(Async(
            move |ctx| { // Pass closure to JS engine
                let rx = rx.clone();
                async move { // Returns future when called
                    if let Some(msg) = {
                        rx.lock().map_err(|_e| Exception::throw_message(&ctx, "Mutex Error"))?.recv().await
                    } {
                        Ok::<String,rquickjs::Error>(msg)
                    } else {
                        Err::<String,rquickjs::Error>(Exception::throw_message(&ctx, "RX Channel Closed"))
                    }
                }
            }
        )))?;
        */

        if let Some(script) = script {
            // Declare module
            let module = Module::declare(ctx.clone(), "main.mjs", script)
                .catch(&ctx)
                .map_err(|e| anyhow!("JS error [declare]: {}", e))?;

            // Evaluate module
            let (_module, promise) = module.eval()
                .catch(&ctx)
                .map_err(|e| anyhow!("JS error [eval]: {}", e))?;

            // Complete promise as future
            promise.into_future::<()>().await
                .catch(&ctx)
                .map_err(|e| anyhow!("JS error [await]: {}", e))?;
        } else {
            // Simple REPL (use ctx.eval to maintain state so no top level await)
            let stdin = tokio::io::stdin();
            let mut reader = BufReader::new(stdin);
            loop {
                let script = read_multiline_input(&mut reader).await?;
                if !script.is_empty() {
                    match ctx.eval::<Value,_>(script) {
                        Ok(v) => {
                            println!("=== {:?}", v);
                            ctx.globals().set("_",v.clone())?;
                        }
                        Err(e) => {
                            if let Ok(ex) = Exception::from_value(ctx.catch()) {
                                eprintln!(
                                    "{}\n{}",
                                    ex.message().unwrap_or("-".into()),
                                    ex.stack().unwrap_or("-".into())
                                );
                            } else {
                                eprintln!("JS Error: {e}");
                            }
                        }
                    }
                }
            }
        }

        Ok::<(),anyhow::Error>(())
    })
    .await?;

    println!(">> Tasks Pending: {:?}", rt.is_job_pending().await);

    rt.idle().await;

    println!(
        "Channel RX: {}",
        match timeout(Duration::from_secs(2), resolve_rx).await {
            Ok(Ok(m)) => m,
            Ok(Err(e)) => format!("Oneshot Err: {e}"),
            Err(_) => "Timeout".into(),
        }
    );

    Ok(())
}

async fn read_multiline_input(reader: &mut BufReader<tokio::io::Stdin>) -> anyhow::Result<String> {
    let mut lines = Vec::new();
    let mut buffer = String::new();

    loop {
        let prompt = if lines.is_empty() { ">>> " } else { "... " };
        print!("{}", prompt);
        std::io::stdout().flush()?;

        buffer.clear();
        reader.read_line(&mut buffer).await?;
        let line = buffer.trim_end();

        lines.push(line.to_string());

        let full_input = lines.join("\n");
        // Check if we need more input (unmatched braces/parens)
        if !needs_more_input(&full_input) {
            return Ok(full_input);
        }
    }
}

fn needs_more_input(input: &str) -> bool {
    let mut balance = 0i32;
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '{' | '(' | '[' => balance += 1,
            '}' | ')' | ']' => {
                balance -= 1;
                if balance < 0 {
                    return false;
                } // Syntax error, but let Rhai handle it
            }
            '"' | '\'' => {
                // Skip string literals
                let quote = ch;
                while let Some(c) = chars.next() {
                    if c == '\\' {
                        // Skip escaped chars
                        chars.next();
                    } else if c == quote {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    balance > 0
}
