use anyhow::anyhow;
use rquickjs::function::{Async, Func};
use rquickjs::{
    async_with, AsyncContext, AsyncRuntime, CatchResultExt, Ctx, Exception,
    Module, Value,
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

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let (resolve_tx, resolve_rx) = tokio::sync::oneshot::channel::<String>();

    tokio::spawn(async move {
        for n in 0..10 {
            match tx.send(format!("Send [{n}]")) {
                Ok(_) => println!("Sent Message: [{n}]"),
                Err(e) => eprintln!("Error Sending Message: {e}"),
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });

    async_with!(ctx => |ctx| {
        // Setup your functions
        ctx.globals().set("print", js_print)?;
        ctx.globals().set("print_v", js_print_v)?;
        ctx.globals().set("sleep", js_sleep)?;

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
