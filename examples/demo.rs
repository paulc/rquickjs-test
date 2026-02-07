use argh::FromArgs;

use rquickjs::{async_with, AsyncContext, AsyncRuntime, Class, Module};
use rquickjs_test::run::{get_script, repl, run_module, run_script};
use rquickjs_test::util::{
    register_fns, register_oneshot, register_rx_channel, register_tx_channel,
};

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
