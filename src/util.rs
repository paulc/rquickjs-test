use rquickjs::{
    function::Rest,
    function::{Async, Func},
    Ctx, Exception, Function, Object, Value,
};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;
use tokio::time::Duration;

/// Register TX channel
pub fn register_oneshot<'js, T>(
    ctx: Ctx<'js>,
    tx: oneshot::Sender<T>,
    f: &str,
) -> anyhow::Result<()>
where
    T: rquickjs::IntoJs<'js> + rquickjs::FromJs<'js> + Clone + Send + 'static,
{
    let tx = Arc::new(Mutex::new(Some(tx)));
    ctx.globals().set(
        f,
        Func::new(move |ctx, msg: T| match tx.lock() {
            Ok(mut guard) => match guard.take() {
                Some(tx) => match tx.send(msg) {
                    Ok(_) => Ok::<(), rquickjs::Error>(()),
                    Err(_) => Err::<(), rquickjs::Error>(Exception::throw_message(
                        &ctx,
                        "TX Channel Closed",
                    )),
                },
                None => {
                    Err::<(), rquickjs::Error>(Exception::throw_message(&ctx, "Already Resolved"))
                }
            },
            Err(_) => Err::<(), rquickjs::Error>(Exception::throw_message(&ctx, "Mutex Error")),
        }),
    )?;
    Ok(())
}

/// Register TX channel
pub fn register_tx_channel<'js, T>(
    ctx: Ctx<'js>,
    tx: UnboundedSender<T>,
    f: &str,
) -> anyhow::Result<()>
where
    T: rquickjs::IntoJs<'js> + rquickjs::FromJs<'js> + Clone + Send + 'static,
{
    let tx = Arc::new(Mutex::new(tx));
    ctx.globals().set(
        f,
        Func::new(Async(move |ctx, msg: T| {
            let tx = tx.clone();
            async move {
                match tx
                    .lock()
                    .map_err(|_| Exception::throw_message(&ctx, "Mutex Error"))?
                    .send(msg)
                {
                    Ok(_) => Ok::<(), rquickjs::Error>(()),
                    Err(_) => Err::<(), rquickjs::Error>(Exception::throw_message(
                        &ctx,
                        "TX Channel Closed",
                    )),
                }
            }
        })),
    )?;
    Ok(())
}

/// Register RX channel
pub fn register_rx_channel<'js, T>(
    ctx: Ctx<'js>,
    rx: UnboundedReceiver<T>,
    f: &str,
) -> anyhow::Result<()>
where
    T: rquickjs::IntoJs<'js> + rquickjs::FromJs<'js> + Clone + Send + 'static,
{
    let rx = Arc::new(Mutex::new(rx));
    ctx.globals().set(
        f,
        Func::new(Async(move |ctx| {
            // Pass closure to JS engine
            let rx = rx.clone();
            async move {
                // Returns future when called
                if let Some(msg) = {
                    rx.lock()
                        .map_err(|_e| Exception::throw_message(&ctx, "Mutex Error"))?
                        .recv()
                        .await
                } {
                    Ok::<T, rquickjs::Error>(msg)
                } else {
                    Err::<T, rquickjs::Error>(Exception::throw_message(&ctx, "RX Channel Closed"))
                }
            }
        })),
    )?;
    Ok(())
}

/// Register useful QJS functions
pub fn register_fns(ctx: &Ctx<'_>) -> anyhow::Result<()> {
    ctx.globals().set("__print", js_print)?;
    ctx.globals().set("__print_v", js_print_v)?;
    ctx.globals().set("__sleep", js_sleep)?;
    ctx.globals().set("__globals", js_globals)?;
    ctx.globals().set("setTimeout", js_set_timeout)?;
    // Add console.log function
    let console = Object::new(ctx.clone())?;
    console.set("log", js_log)?;
    ctx.globals()
        .get::<_, Object>("globalThis")?
        .set("console", console)?;
    Ok(())
}

/// Print JS String
#[rquickjs::function]
fn print(s: String) {
    println!("{}", s);
}

/// Convert Value to JSON String
pub fn value_to_json<'js>(ctx: Ctx<'js>, v: Value<'js>) -> anyhow::Result<String> {
    if v.is_undefined() {
        Ok("null".into())
    } else {
        ctx.json_stringify(v)?
            .and_then(|s| s.as_string().map(|s| s.to_string().ok()).flatten())
            .ok_or(anyhow::anyhow!("JSON Error"))
    }
}

/// Convert JSON String to Value
pub fn json_to_value<'js>(ctx: Ctx<'js>, json: &str) -> anyhow::Result<Value<'js>> {
    match ctx.json_parse(json.as_bytes()) {
        Ok(v) => Ok(v),
        Err(e) => {
            if let Ok(ex) = rquickjs::Exception::from_value(ctx.catch()) {
                Err(anyhow::anyhow!(
                    "JSON Error: {}\n{}",
                    ex.message().unwrap_or("-".into()),
                    ex.stack().unwrap_or("-".into())
                ))
            } else {
                Err(anyhow::anyhow!("JSON Error: {e}"))
            }
        }
    }
}

/// Print JS Value as JSON
#[rquickjs::function]
pub fn print_v<'js>(ctx: Ctx<'js>, v: Value<'js>) -> rquickjs::Result<()> {
    let output = ctx
        .json_stringify(v)?
        .and_then(|s| s.as_string().map(|s| s.to_string().ok()).flatten())
        .unwrap_or_else(|| "<ERR>".to_string());
    println!("{}", output);
    Ok(())
}

/// Print globals
#[rquickjs::function]
fn globals<'js>(ctx: Ctx<'js>) -> rquickjs::Result<()> {
    let mut i = ctx.globals().props::<String, rquickjs::Value>();
    while let Some(Ok((k, v))) = i.next() {
        println!("{} v = {:?}", k, v);
    }
    Ok(())
}

/// console.log
#[rquickjs::function]
fn log<'js>(ctx: Ctx<'js>, args: Rest<Value<'js>>) -> rquickjs::Result<()> {
    println!(
        "{}",
        args.iter()
            .map(|a| -> rquickjs::Result<String> {
                Ok(ctx
                    .json_stringify(a)?
                    .and_then(|s| s.as_string().map(|s| s.to_string().ok()).flatten())
                    .unwrap_or_else(|| "<ERR>".to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?
            .join(", ")
    );
    Ok(())
}

#[rquickjs::function]
async fn sleep(n: u64) -> rquickjs::Result<()> {
    tokio::time::sleep(Duration::from_secs(n)).await;
    Ok(())
}

#[rquickjs::function]
async fn set_timeout<'js>(
    ctx: Ctx<'js>,
    n: u64,
    f: Function<'js>,
    args: Rest<Value<'js>>,
) -> rquickjs::Result<()> {
    tokio::time::sleep(Duration::from_secs(n)).await;
    let mut arg = rquickjs::function::Args::new(ctx.clone(), args.len());
    arg.push_args(args.iter())?;
    f.call_arg(arg)
}
