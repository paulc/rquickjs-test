use anyhow::anyhow;
use rquickjs::{
    async_with,
    function::{Async, Func},
    AsyncContext, AsyncRuntime, CatchResultExt, Exception, Module, Value,
};

use tokio::time::{sleep, Duration};

fn print(s: String) {
    println!("{}", s);
}

async fn async_stuff() -> String {
    "STUFF".into()
}

// .map_err(|_| handle_js_exception(ctx.catch()))?;
fn _handle_js_exception(e: Value) -> String {
    if let Ok(ex) = Exception::from_value(e) {
        let msg = ex.message().unwrap_or_default();
        let stack = ex.stack().unwrap_or_default();
        format!("Syntax error - {}\n{}", msg, stack)
    } else {
        format!("Unknown Error")
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let rt = AsyncRuntime::new()?;
    let ctx = AsyncContext::full(&rt).await?;

    let script = r#"
                print("start sync");
                print(await async_stuff());
                async_closure(5).then(() => resolve("DONE"));
                print("end sync");
            "#;

    // let (tx, rx) = tokio::sync::oneshot::channel();
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);

    let _: anyhow::Result<()> = async_with!(ctx => |ctx| {
        // Setup your functions
        ctx.globals().set("print", Func::new(print))?;
        ctx.globals().set("async_stuff", Func::new(Async(async_stuff)))?;

        // Need to make sure that closure is Fn not FnOnce
        let v = "MOVED".to_string();
        ctx.globals().set("async_closure", Func::new(Async(move |n: usize| {
            let v = v.clone();
            async move {
                for i in 0..n {
                    sleep(Duration::from_secs(1)).await;
                    println!(">>{} [{}]", v, i);
                }
            }
        })))?;

        // With oneshot need to wrap tx to make sure closure is Fn vs FnOnce (send consumes tx)
        // let tx = std::sync::Mutex::new(Some(tx));
        // ctx.globals().set("resolve", Func::new(move |result: String| {
        //     if let Ok(mut guard) = tx.lock() {
        //         if let Some(tx) = guard.take() {
        //             let _ = tx.send(result);
        //         }
        //     }
        // }))?;
        ctx.globals().set("resolve", Func::new(move |result: String| {
            let _ = tx.try_send(result);
        }))?;

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

        Ok(())
    })
    .await;

    // Wait for async tasks to complete
    rt.idle().await;

    println!("Channel RX: {:?}", rx.recv().await);

    Ok(())
}
