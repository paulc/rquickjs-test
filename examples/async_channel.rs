use rquickjs::{async_with, AsyncContext, AsyncRuntime, Ctx, Exception, Function, Value};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), rquickjs::Error> {
    let rt = AsyncRuntime::new().unwrap();
    let ctx = AsyncContext::full(&rt).await.unwrap();

    async_with!(ctx => |ctx| {
        // Create async channel
        let (tx, _rx) = mpsc::channel::<String>(100);

        let send_fn = Function::new(ctx.clone(), move |ctx: Ctx, msg: String| -> Result<(), rquickjs::Error>{
            let (promise, resolve, reject) = ctx.promise()?;
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

        /*
        let rx_fn = Function::new(ctx.clone(), move |ctx: Ctx| -> Result<Promise, rquickjs::Error>{
            let (promise, resolve, reject) = ctx.promise()?;
            ctx.spawn(async move {
                match rx.recv().await {
                    Some(_) => {
                        // Resolve the promise
                        let mut args = Args::new(ctx,1);
                        args.push_arg("RX OK".to_string());
                        reject.call_arg(args);
                    }
                    None => {
                        // Reject the promise
                        let mut args = Args::new(ctx,1);
                        args.push_arg("RX Err".to_string());
                        reject.call_arg(args);
                    }
                }
            });
            Ok(promise)
        }).unwrap();

        // Move receiver
        let receive_fn = Function::new(ctx.clone(), move |ctx: Ctx| -> Result<String, rquickjs::Error> {
            rx.try_recv().map_err(|e| { Exception::throw_message(&ctx, &e.to_string()) })
            }
        ).unwrap();
        */


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
            print("JS")
            // send("Hello from JS");  
            const r = recv();
            print(`>> RECV: ${r}`);
            // const msg = tryReceive();  
            // if (msg) {  
            //     print(`Received: ${msg}`);  
            // }  
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
