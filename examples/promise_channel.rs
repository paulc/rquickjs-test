use rquickjs::{async_with, AsyncContext, AsyncRuntime, Ctx, Exception, Function, Value};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), rquickjs::Error> {
    let rt = AsyncRuntime::new().unwrap();
    let ctx = AsyncContext::full(&rt).await.unwrap();

    async_with!(ctx => |ctx| {
        // Create async channel
        let (tx, mut rx) = mpsc::channel::<String>(100);

        // Construct Promise manually
        let send_fn = Function::new(ctx.clone(), move |ctx: Ctx, msg: String| -> Result<(), rquickjs::Error>{
            let (_promise, resolve, reject) = ctx.promise()?;
            let tx = tx.clone();
            ctx.spawn(async move {
                match tx.send(msg).await {
                    Ok(_) => {
                        // Resolve the promise
                        let _ = resolve.call::<_,()>(("Send OK",));
                    }
                    Err(_e) => {
                        // Reject the promise
                        let _ = reject.call::<_,()>(("Send Err",));
                    }
                }
            });
            Ok(())
        }).unwrap();

        let global = ctx.globals();
        global.set("send", send_fn).unwrap();
        // global.set("recv", receive_fn).unwrap();
        global.set(
            "print",
            Function::new(ctx.clone(), |v: Value| {
                println!("{:?}", v);
            }),
        ).unwrap();

        // Use the functions from JavaScript
        let _res = ctx.eval::<(), _>(r#"
            send("Hello from JS");  
        "#);  

        if let Ok(ex) = Exception::from_value(ctx.catch()) {
            println!(
                "{}\n{}",
                ex.message().unwrap_or("-".into()),
                ex.stack().unwrap_or("-".into())
            );
        }

        println!("RX >> {:?}", rx.recv().await);

    })
    .await;

    rt.idle().await;
    println!("DONE");
    Ok(())
}
