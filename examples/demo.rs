use argh::FromArgs;

use rquickjs::{async_with, AsyncContext, AsyncRuntime, Class, Module};
use rquickjs_test::run::{call_fn, get_script, repl_rustyline, run_module, run_script};
use rquickjs_test::util::{
    json_to_value, register_fns, register_oneshot, register_rx_channel, register_tx_channel,
    value_to_json,
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

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let (tx2, rx2) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();

    tokio::spawn(async move {
        let mut n = 0_usize;
        loop {
            match rx.recv().await {
                Some(msg) => {
                    println!("[RECV] -> {msg:?}");
                    let msg = format!("MSG [{}]", n).as_bytes().to_vec();
                    println!("[SEND] -> {:?}", tx2.send(msg));
                    n += 1;
                }
                None => {
                    eprintln!(">> RX Channel Closed");
                    break;
                }
            }
        }
    });

    let (oneshot_tx, oneshot_rx) = tokio::sync::oneshot::channel::<Stuff>();

    tokio::spawn(async move {
        match oneshot_rx.await {
            Ok(msg) => {
                println!("[RESOLVED] -> {msg:?}");
            }
            Err(_) => eprintln!(">> Oneshot Channel Closed"),
        }
    });

    async_with!(ctx => |ctx| {
        register_fns(&ctx)?;
        register_tx_channel(ctx.clone(), tx, "send")?;
        register_rx_channel(ctx.clone(), rx2, "recv")?;
        register_oneshot(ctx.clone(), oneshot_tx, "resolve")?;
        Class::<Stuff>::define(&ctx.globals())?;

        let (_, p) = Module::evaluate_def::<js_test_mod,_>(ctx.clone(),"stuff")?;
        p.into_future::<()>().await?; // Ensure module evaluated

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
            repl_rustyline(ctx.clone()).await?;
        }

        // Call JS
        for (f,a) in args.call.iter().zip(args.arg.iter().chain(std::iter::repeat(&("".to_string())))) {
            let r = if a.is_empty() {
                call_fn(ctx.clone(),&f,((),)).await?
            } else {
                call_fn(ctx.clone(),&f,(json_to_value(ctx.clone(),a)?,)).await?
            };
            println!(">> [CALL] {f} ({a}) => {}", value_to_json(ctx.clone(),r)?);
        }
        Ok::<(),anyhow::Error>(())
    })
    .await?;

    println!(">> Tasks Pending: {:?}", rt.is_job_pending().await);

    rt.idle().await;

    Ok(())
}

#[derive(Debug, Clone, rquickjs::class::Trace, rquickjs::JsLifetime)]
#[rquickjs::class]
struct Stuff {
    #[qjs(get, set)]
    a: Option<String>,
    #[qjs(get, set)]
    b: u64,
}

#[rquickjs::methods]
impl Stuff {
    #[qjs(constructor)]
    pub fn new() -> Self {
        Self { a: None, b: 0 }
    }
}

#[rquickjs::module]
mod test_mod {
    #[rquickjs::function]
    pub fn hello() -> String {
        "HELLO".to_string()
    }

    #[rquickjs::function]
    pub async fn async_hello() -> String {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        "HELLO".to_string()
    }

    #[derive(Debug, Clone, rquickjs::class::Trace, rquickjs::JsLifetime)]
    #[rquickjs::class()]
    pub struct Stuff2 {
        #[qjs(get, set)]
        a: Option<String>,
        #[qjs(get, set)]
        b: u64,
    }

    #[rquickjs::methods]
    impl Stuff2 {
        #[qjs(constructor)]
        pub fn new() -> Self {
            Self { a: None, b: 0 }
        }
    }
}
