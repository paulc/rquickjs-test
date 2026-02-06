use rquickjs::{
    function::{Async, Func},
    Ctx, Exception, Value,
};
use std::io::Read;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::time::Duration;

/// Expand script to handle literal script, @file or stdin (-)
pub fn get_script(script: &str) -> anyhow::Result<String> {
    Ok(if script == "-" {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        s
    } else if script.starts_with("@") {
        std::fs::read_to_string(&script[1..])?
    } else {
        script.to_string()
    })
}

/// Register TX channel
/*
pub fn register_tx_channel<'js, T>(ctx: Ctx<'js>, f: &str) -> anyhow::Result<UnboundedReceiver<T>>
where
    T: rquickjs::IntoJs<'js> + rquickjs::FromJs<'js> + Clone + Send + 'static,
{
    let (tx, rx) = unbounded_channel::<T>();
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
    Ok(rx)
}
*/

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
    ctx.globals().set("print", js_print)?;
    ctx.globals().set("print_v", js_print_v)?;
    ctx.globals().set("sleep", js_sleep)?;
    Ok(())
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
