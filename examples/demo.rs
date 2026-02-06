use anyhow::anyhow;
use argh::FromArgs;

use rquickjs::{async_with, AsyncContext, AsyncRuntime, CatchResultExt, Module};
use rquickjs_test::repl::repl;
use rquickjs_test::util::{get_script, register_fns, register_rx_channel, register_tx_channel};

use tokio::time::{sleep, Duration};

#[derive(FromArgs)]
/// CLI Args
struct CliArgs {
    #[argh(option)]
    /// QJS script
    script: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: CliArgs = argh::from_env();

    let script = match args.script {
        Some(s) => Some(get_script(&s)?),
        None => None,
    };

    let rt = AsyncRuntime::new()?;
    let ctx = AsyncContext::full(&rt).await?;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Some(msg) => println!("[RECV] -> {msg:?}"),
                None => {
                    eprintln!("Channel Closed");
                    break;
                }
            }
        }
    });

    let (tx2, rx2) = tokio::sync::mpsc::unbounded_channel::<String>();

    tokio::spawn(async move {
        let mut n = 0_usize;
        loop {
            sleep(Duration::from_secs(2)).await;
            println!("[SEND] -> {:?}", tx2.send(format!("SEND [{n}]")));
            n += 1;
        }
    });

    async_with!(ctx => |ctx| {
        register_fns(&ctx)?;
        register_tx_channel(ctx.clone(), tx, "send")?;
        register_rx_channel(ctx.clone(), rx2, "recv")?;

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
            repl(ctx).await?;
        }
        Ok::<(),anyhow::Error>(())
    })
    .await?;

    println!(">> Tasks Pending: {:?}", rt.is_job_pending().await);

    rt.idle().await;

    Ok(())
}
