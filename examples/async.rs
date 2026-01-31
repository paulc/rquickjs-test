use rquickjs::{async_with, prelude::*, AsyncContext, AsyncRuntime, Function, Result};
use std::time::Duration;

async fn run() {
    let rt = AsyncRuntime::new().unwrap();
    let ctx = AsyncContext::full(&rt).await.unwrap();

    // In order for futures to convert to JavaScript promises they need to return `Result`.
    async fn delay<'js>(amount: f64, cb: Function<'js>) -> Result<()> {
        tokio::time::sleep(Duration::from_secs_f64(amount)).await;
        let _ = cb.call::<(), ()>(());
        Ok(())
    }

    fn print(text: String) -> Result<()> {
        println!("{}", text);
        Ok(())
    }

    let mut some_var = 1;
    // closure always moves, so create a ref.
    let some_var_ref = &mut some_var;
    async_with!(ctx => |ctx|{

        // With the macro you can borrow the environment.
        *some_var_ref += 1;

        let delay = Function::new(ctx.clone(),Async(delay))
            .unwrap()
            .with_name("print")
            .unwrap();

        let global = ctx.globals();
        global.set("print",Func::from(print)).unwrap();
        global.set("delay",delay).unwrap();
        ctx.eval::<(),_>(r#"
            print("start");  
            delay(1,() => {  
                print("delayed");  
            })  
            print("after");  
        "#).unwrap();  
    })
    .await;
    assert_eq!(some_var, 2);

    rt.idle().await
}

#[tokio::main]
async fn main() {
    run().await;
}
