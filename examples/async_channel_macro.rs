use anyhow::anyhow;
use rquickjs::class::Trace;
use rquickjs::function::Func;
use rquickjs::{
    async_with, ArrayBuffer, AsyncContext, AsyncRuntime, CatchResultExt, Class, Ctx, JsLifetime,
    Module, Value,
};

use tokio::time::{timeout, Duration};

#[derive(Trace, JsLifetime, Clone, Debug)]
#[rquickjs::class]
pub struct TestClass {
    #[qjs(get, set)]
    pub name: String,
    pub data: Vec<u8>,
}

#[rquickjs::methods]
impl TestClass {
    #[qjs(get, rename = "data")]
    pub fn get_data<'js>(&self, ctx: Ctx<'js>) -> rquickjs::Result<ArrayBuffer<'js>> {
        ArrayBuffer::new(ctx.clone(), self.data.clone())
    }
    #[qjs(get, rename = "text")]
    pub fn get_text(&self) -> rquickjs::Result<String> {
        Ok(String::from_utf8(self.data.clone())?)
    }
    #[qjs(set, rename = "text")]
    pub fn set_text(&mut self, text: String) -> rquickjs::Result<()> {
        self.data = text.as_bytes().to_vec();
        Ok(())
    }
    pub fn format(&self) -> rquickjs::Result<String> {
        Ok(format!("{:?}", self))
    }
}

#[derive(Trace, JsLifetime, Clone, Debug)]
#[rquickjs::class]
pub enum TestEnum {
    A(u8),
    B(u8),
}

#[rquickjs::methods]
impl TestEnum {
    pub fn get_type(&self) -> rquickjs::Result<String> {
        match self {
            TestEnum::A(_) => Ok("A".into()),
            TestEnum::B(_) => Ok("B".into()),
        }
    }
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let rt = AsyncRuntime::new()?;
    let ctx = AsyncContext::full(&rt).await?;

    tokio::spawn(async {
        let mut n = 0_usize;
        loop {
            println!("Tick [{n}]");
            tokio::time::sleep(Duration::from_secs(1)).await;
            n += 1;
        }
    });

    let script = r#"
                const o = { a:1, b:[1,2,3], c:false };
                print("start sync");
                print_v(e.get_type());
                print_v(t.data.byteLength);
                print_v(t.text);
                const v = new Int8Array(t.data);
                print_v(v);
                t.text = "CHANGED";
                print(t.format());
                print("end sync");
                export const result = "DONE";
            "#;

    let (resolve_tx, resolve_rx) = tokio::sync::oneshot::channel();

    async_with!(ctx => |ctx| {
        // Setup your functions
        ctx.globals().set("print", js_print)?;
        ctx.globals().set("print_v", js_print_v)?;
        ctx.globals().set("sleep", js_sleep)?;

        let cls = Class::instance(
            ctx.clone(),
            TestClass {
                name: "Test".into(),
                data: "HELLO".as_bytes().to_vec(),
            },
        );

        let e = TestEnum::A(99);

        ctx.globals().set("t", cls)?;
        ctx.globals().set("e", e)?;


        // With oneshot need to wrap tx to make sure closure is Fn vs FnOnce (send consumes tx)
        let resolve_tx = std::sync::Mutex::new(Some(resolve_tx));
        ctx.globals().set("resolve", Func::new(move |result: String| {
            if let Ok(mut guard) = resolve_tx.lock() {
                if let Some(resolve_tx) = guard.take() {
                    let _ = resolve_tx.send(result);
                }
            }
        }))?;

        // Declare module
        let module = Module::declare(ctx.clone(), "main.mjs", script)
            .catch(&ctx)
            .map_err(|e| anyhow!("JS error [declare]: {}", e))?;

        // Evaluate module
        let (module, promise) = module.eval()
            .catch(&ctx)
            .map_err(|e| anyhow!("JS error [eval]: {}", e))?;

        // Complete promise as future
        promise.into_future::<()>().await
            .catch(&ctx)
            .map_err(|e| anyhow!("JS error [await]: {}", e))?;

        println!("RESULT = {:?}", module.get::<_,String>("result")?);

        Ok::<(),anyhow::Error>(())
    })
    .await?;

    println!(">> Tasks Pending: {:?}", rt.is_job_pending().await);

    rt.idle().await;

    println!(
        "Channel RX: {}",
        match timeout(Duration::from_secs(2), resolve_rx).await {
            Ok(Ok(m)) => m,
            Ok(Err(e)) => format!("Oneshot Err: {e}"),
            Err(_) => "Timeout".into(),
        }
    );

    Ok(())
}
