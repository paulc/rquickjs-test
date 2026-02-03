use anyhow::anyhow;
use rquickjs::class::Trace;
use rquickjs::function::{Async, Func};
use rquickjs::{
    async_with, ArrayBuffer, AsyncContext, AsyncRuntime, CatchResultExt, Class, Ctx, Exception,
    JsLifetime, Module, Value,
};

use std::io::Read;
use tokio::time::{timeout, Duration};

use argh::FromArgs;

#[derive(FromArgs)]
/// Async Channel
struct CliArgs {
    #[argh(option)]
    /// QJS script
    script: String,
}

#[derive(Trace, JsLifetime, Clone, Debug)]
#[rquickjs::class]
pub struct TestClass {
    #[qjs(get, set)]
    pub name: String,
    pub data: Vec<u8>,
}

#[rquickjs::methods]
impl TestClass {
    #[qjs(get, rename = "data")]
    pub fn get_data<'js>(&self, ctx: Ctx<'js>) -> rquickjs::Result<ArrayBuffer<'js>> {
        ArrayBuffer::new(ctx.clone(), self.data.clone())
    }
    #[qjs(get, rename = "text")]
    pub fn get_text(&self) -> rquickjs::Result<String> {
        Ok(String::from_utf8(self.data.clone())?)
    }
    #[qjs(set, rename = "text")]
    pub fn set_text(&mut self, text: String) -> rquickjs::Result<()> {
        self.data = text.as_bytes().to_vec();
        Ok(())
    }
    pub fn format(&self) -> rquickjs::Result<String> {
        Ok(format!("{:?}", self))
    }
}

#[derive(Trace, JsLifetime, Clone, Debug)]
#[rquickjs::class]
pub enum TestEnum {
    A(u8),
    B(u8),
}

#[rquickjs::methods]
impl TestEnum {
    pub fn get_type(&self) -> rquickjs::Result<String> {
        match self {
            TestEnum::A(_) => Ok("A".into()),
            TestEnum::B(_) => Ok("B".into()),
        }
    }
}

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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: CliArgs = argh::from_env();

    let script = if args.script == "-" {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        s
    } else if args.script.starts_with("@") {
        std::fs::read_to_string(&args.script[1..])?
    } else {
        args.script
    };

    let rt = AsyncRuntime::new()?;
    let ctx = AsyncContext::full(&rt).await?;

    /*
    let script = r#"
                const o = { a:1, b:[1,2,3], c:false };
                print("start sync");
                print_v(e.get_type());
                print_v(t.data.byteLength);
                print_v(t.text);
                const v = new Int8Array(t.data);
                print_v(v);
                t.text = "CHANGED";
                print(t.format());
                sleep(1).then(() => resolve("RESOLVED"));
                print("end sync");
                export const result = "DONE";
            "#;
    */

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let (resolve_tx, resolve_rx) = tokio::sync::oneshot::channel::<String>();

    tokio::spawn(async {
        let mut n = 0_usize;
        loop {
            println!("Tick [{n}]");
            tokio::time::sleep(Duration::from_secs(1)).await;
            n += 1;
        }
    });

    tokio::spawn(async move {
        let mut n = 0_usize;
        loop {
            match tx.send(format!("Send [{n}]")) {
                Ok(_) => println!("Sent Message: [{n}]"),
                Err(e) => eprintln!("Error Sending Message: {e}"),
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
            n += 1;
        }
    });

    async_with!(ctx => |ctx| {
        // Setup your functions
        ctx.globals().set("print", js_print)?;
        ctx.globals().set("print_v", js_print_v)?;
        ctx.globals().set("sleep", js_sleep)?;

        let cls = Class::instance(
            ctx.clone(),
            TestClass {
                name: "Test".into(),
                data: "HELLO".as_bytes().to_vec(),
            },
        );

        let e = TestEnum::A(99);

        ctx.globals().set("t", cls)?;
        ctx.globals().set("e", e)?;


        // With oneshot need to wrap tx to make sure closure is Fn vs FnOnce (send consumes tx)
        let resolve_tx = std::sync::Mutex::new(Some(resolve_tx));
        ctx.globals().set("resolve", Func::new(move |result: String| {
            if let Ok(mut guard) = resolve_tx.lock() {
                if let Some(resolve_tx) = guard.take() {
                    let _ = resolve_tx.send(result);
                }
            }
        }))?;

        // Make sure rx is Copy (Fn vs FnOnce)
        let rx = std::sync::Arc::new(std::sync::Mutex::new(rx)); 
        ctx.globals().set("msg_rx", Func::new(Async({
            let rx = rx.clone();
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
        })))?;

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
