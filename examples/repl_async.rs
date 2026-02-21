use argh::FromArgs;

use rquickjs::{async_with, AsyncContext, AsyncRuntime};
use rquickjs_test::run::{call_fn, get_script, repl_rl, run_module, run_script};
use rquickjs_test::util::{
    json_to_value, register_fns, register_oneshot, register_tx_channel, value_to_json,
};

#[derive(FromArgs)]
/// CLI Args
struct CliArgs {
    #[argh(option)]
    /// QJS script
    script: Vec<String>,
    #[argh(option)]
    /// QJS module
    module: Vec<String>,
    #[argh(switch)]
    /// JS REPL
    repl: bool,
    #[argh(option)]
    /// call JS
    call: Vec<String>,
    #[argh(option)]
    /// call args
    arg: Vec<String>,
}

/// Basic CLI test
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: CliArgs = argh::from_env();

    // Check that we have something to do
    if args.script.is_empty() && args.module.is_empty() && args.call.is_empty() && !args.repl {
        let name = std::env::args().next().unwrap_or("-".into());
        CliArgs::from_args(&[&name], &["--help"]).map_err(|exit| anyhow::anyhow!(exit.output))?;
    }

    let rt = AsyncRuntime::new()?;
    let ctx = AsyncContext::full(&rt).await?;

    let (oneshot_tx, oneshot_rx) = tokio::sync::oneshot::channel::<String>();

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Some(m) => println!("RX Msg: {m}"),
                None => {
                    println!("[-] RX Channel Closed");
                    break;
                }
            }
        }
    });

    tokio::spawn(async move {
        match oneshot_rx.await {
            Ok(msg) => {
                println!("[+] Oneshot Resolved -> {msg}");
            }
            Err(_) => eprintln!("[-] Oneshot Channel Closed"),
        }
    });

    async_with!(ctx => |ctx| {
        register_fns(&ctx)?;
        register_oneshot(ctx.clone(), oneshot_tx, "resolve")?;
        register_tx_channel(ctx.clone(), tx, "send")?;

        // Run modules
        for module in args.module {
            run_module(ctx.clone(),get_script(&module)?).await?;
        }

        // Run scripts
        for script in args.script {
            run_script(ctx.clone(),get_script(&script)?).await?;
        }

        // Run REPL
        if args.repl {
            repl_rl(ctx.clone()).await?;
        }

        // Call JS
        for (f,a) in args.call.iter().zip(args.arg.iter().chain(std::iter::repeat(&("".to_string())))) {
            let r = if a.is_empty() {
                call_fn(ctx.clone(),&f,((),)).await?
            } else {
                call_fn(ctx.clone(),&f,(json_to_value(ctx.clone(),a)?,)).await?
            };
            println!("[+] Call: {f}({a}) => {}", value_to_json(ctx.clone(),r)?);
        }
        Ok::<(),anyhow::Error>(())
    })
    .await?;

    println!("[+] Tasks Pending: {:?}", rt.is_job_pending().await);

    rt.idle().await;

    Ok(())
}
