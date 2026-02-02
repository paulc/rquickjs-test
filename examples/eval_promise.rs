use rquickjs::{
    async_with,
    function::{Async, Func},
    AsyncContext, AsyncRuntime, Exception,
};

fn zark(n: usize) -> Result<(), rquickjs::Error> {
    (0..n).for_each(|n| println!("Zark [{}]", n));
    Ok(())
}

fn print(s: String) {
    println!("{}", s);
}

async fn async_stuff() -> String {
    "STUFF".into()
}

#[tokio::main]
async fn main() -> Result<(), rquickjs::Error> {
    let rt = AsyncRuntime::new()?;
    let ctx = AsyncContext::full(&rt).await?;

    let res: Result<(), String> = async_with!(ctx => |ctx| {

        ctx.globals().set("greeting", "HELLO").map_err(|e| format!("{}", e))?;
        ctx.globals().set("print", Func::new(print)).map_err(|e| format!("{}", e))?;
        ctx.globals().set("zark", Func::new(zark)).map_err(|e| format!("{}", e))?;
        ctx.globals().set("async_stuff", Func::new(Async(async_stuff))).map_err(|e| format!("{}", e))?;
        // XXX Error moving value into closure
        ctx.globals().set("async_closure", Func::new(Async(async move || {
            "ASYNC_CLOSURE".to_string()
        }))).map_err(|e| format!("{}", e))?;

        // ctx.eval::<rquickjs::Promise, _>(...)
        //      - evals script that returns Promise synchronously
        //      - no top level await - use IIFE (async () => { ... })()
        //      - IIFE return Promise)
        // ctx.eval_promise::<_>(...)
        //      - evals until first await then retruns Err(WouldBlock)
        //      - only returns Ok(Promise) is script completes synchronously
        //      - top level await support
        //
        // | Step                   | `eval::<Promise>("(async()=>{})()")`  | `eval_promise("await ...")`     |
        // | ---------------------- | ------------------------------------- | ------------------------------- |
        // | **Script starts**      | Runs immediately                      | Runs immediately                |
        // | **Hits first `await`** | Inside IIFE (scheduled)               | At top level → **WouldBlock**   |
        // | **Returns to Rust**    | Promise handle immediately            | Err(WouldBlock)                 |
        // | **Body execution**     | Deferred to `rt.idle()` or `finish()` | Interupted, needs manual resume |

        // let promise = ctx.eval_promise::<_>(r#"
        let promise = ctx.eval::<rquickjs::Promise, _>(r#"
            (async () => {
                print("start");  
                zark(10);
                async_stuff().then(s => print(`Async: ${s}`));
                async_closure().then(s => print(`Async: ${s}`));
                print(await async_stuff());
                print("end");
            })()
        "#).map_err(|e| format!("?? {}", e))?;

        // promise.finish will return Error::WouldBlock if there are outstanding tasks
        // (we need to poll using rt.idle() outside async_with! block to complete)    
        match promise.finish::<()>() {
            Ok(r) => println!("<Async tasks complete> {:?}", r),
            Err(rquickjs::Error::WouldBlock) => println!("<Async tasks still running>"),
            Err(e) => {
                if let Ok(ex) = Exception::from_value(ctx.catch()) {
                    eprintln!(
                        "Exception: {}\n{}",
                        ex.message().unwrap_or("-".into()),
                        ex.stack().unwrap_or("-".into())
                    )
                } else {
                    eprintln!("Err: {:?}",e);
                }
            }
        }

        Ok(())
    })
    .await;

    rt.idle().await;

    println!("{:?}", res);

    Ok(())
}
