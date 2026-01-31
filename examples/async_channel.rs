use rquickjs::{
    async_with, AsyncContext, AsyncRuntime, CatchResultExt, Exception, Function, Value,
};
use std::sync::mpsc;

fn print(v: Value) {
    println!(">> {v:?}");
}

#[tokio::main]
async fn main() -> Result<(), rquickjs::Error> {
    let rt = AsyncRuntime::new().unwrap();
    let ctx = AsyncContext::full(&rt).await.unwrap();

    async_with!(ctx => |ctx| {
        // Create channel
        let (tx, rx) = mpsc::channel::<String>();

        // Move sender
        let ctx_f = ctx.clone();
        let send_fn = Function::new(ctx.clone(), move |msg: String| {
            tx.send(msg).map_err(|e| {
                Exception::throw_message(&ctx_f, &e.to_string())
            })?;
            Ok::<(),rquickjs::Error>(())
        }).unwrap();

        /*
        // Move receiver
        let receive_fn = Function::new(ctx.clone(), move || -> Result<Option<String>, rquickjs::Error> {
            match rx.try_recv() {
                Ok(msg) => Ok(Some(msg)),
                Err(mpsc::TryRecvError::Empty) => Ok(None),
                Err(mpsc::TryRecvError::Disconnected) => Ok(None),
            }
        }).unwrap();

        ctx.globals().set(
            "print",
            Function::new(ctx.clone(), |v: Value| {
                println!("{:?}", v);
            }),
        ).unwrap();
        */

        let global = ctx.globals();
        global.set("send", send_fn).unwrap();
        global.set("tryReceive", receive_fn).unwrap();

        // Use the functions from JavaScript
        let _res = ctx.eval::<(), _>(r#"
            send("Hello from JS");  
            const msg = tryReceive();  
            if (msg) {  
                print(`Received: ${msg}`);  
            }  
        "#);  

        if let Ok(ex) = Exception::from_value(ctx.catch()) {
            println!(
                "{}\n{}",
                ex.message().unwrap_or("-".into()),
                ex.stack().unwrap_or("-".into())
            );
        }
    })
    .await;

    rt.idle().await;
    println!("DONE");
    Ok(())
}
