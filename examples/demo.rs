use argh::FromArgs;

use rquickjs::{async_with, AsyncContext, AsyncRuntime};
use rquickjs_test::run::{get_script, repl, run_module, run_script};
use rquickjs_test::util::{register_fns, register_rx_channel, register_tx_channel};

// use tokio::time::{sleep, Duration};

#[derive(FromArgs)]
/// CLI Args
struct CliArgs {
    #[argh(option)]
    /// QJS script
    script: Option<String>,
    #[argh(option)]
    /// QJS module
    module: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: CliArgs = argh::from_env();

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

    let (_tx2, rx2) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();

    /*
    tokio::spawn(async move {
        let mut n = 0_usize;
        loop {
            sleep(Duration::from_secs(2)).await;
            let msg = format!("MSG [{}]", n).as_bytes().to_vec();
            println!("[SEND] -> {:?}", tx2.send(msg));
            n += 1;
        }
    });
    */

    async_with!(ctx => |ctx| {
        register_fns(&ctx)?;
        register_tx_channel(ctx.clone(), tx, "send")?;
        register_rx_channel(ctx.clone(), rx2, "recv")?;

        match args {
            CliArgs { script: _, module: Some(module) } => { run_module(ctx,get_script(&module)?).await?; }
            CliArgs { script: Some(script), module: _ } => { run_script(ctx,get_script(&script)?).await?; }
            CliArgs { script: None, module: None } => { repl(ctx).await?; }
        }

        Ok::<(),anyhow::Error>(())
    })
    .await?;

    println!(">> Tasks Pending: {:?}", rt.is_job_pending().await);

    rt.idle().await;

    Ok(())
}
