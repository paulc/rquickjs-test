use anyhow::{anyhow, Result};
use rquickjs::{async_with, AsyncContext, AsyncRuntime, CatchResultExt, Ctx, Function, Module};

#[rquickjs::module]
mod async_module {
    use super::*;

    #[rquickjs::function]
    #[qjs(rename = "fetchData")]
    pub async fn fetch_data() -> Result<String, rquickjs::Error> {
        Ok("Hello from Rust async".to_string())
    }

    #[rquickjs::function]
    #[qjs(rename = "processData")]
    pub async fn process_data(input: String) -> Result<String, rquickjs::Error> {
        Ok(format!("Processed: {}", input))
    }

    #[rquickjs::function]
    pub fn print(s: String) {
        println!("{s}");
    }

    #[rquickjs::function]
    pub fn throw_ex(ctx: Ctx<'_>, b: bool) -> Result<(), rquickjs::Error> {
        match b {
            true => Ok(()),
            false => Err(rquickjs::Exception::throw_message(&ctx, "THROW")),
        }
    }
}

#[rquickjs::function]
pub async fn double(i: i64) -> i64 {
    i * 2
}

#[tokio::main]
async fn main() -> Result<()> {
    let rt = AsyncRuntime::new().unwrap();
    let ctx = AsyncContext::full(&rt).await.unwrap();

    let script = r#"
            import { print, fetchData, processData, throw_ex } from "rust_async_mod";

            print("ASYNC");
            print(zark());
            double(99).then((i) => print(`Double: ${i}`));

            fetchData().then(result => {
                print(result);
                return processData(result);
            }).then(final => {
                print(final);
            });
    "#;

    let r: Result<()> = async_with!(ctx => |ctx| {
        // Declare the Rust module
        Module::declare_def::<js_async_module, _>(ctx.clone(), "rust_async_mod").unwrap();

        ctx.globals().set("double", js_double).unwrap();

        let zark_msg = "ZARK!";
        ctx.globals().set("zark", Function::new(ctx.clone(), move || { zark_msg })).unwrap();

        let m = rquickjs::Module::declare(ctx.clone(), "script", script)
            .catch(&ctx)
            .map_err(|e| anyhow!("JS error [declare]: {}", e))?;
        let (_m, m_promise) = m
            .eval()
            .catch(&ctx)
            .map_err(|e| anyhow!("JS error [eval]: {}", e))?;
        () = m_promise
            .into_future::<()>().await
            .catch(&ctx)
            .map_err(|e| anyhow!("JS error [finish]: {}", e))?;
        Ok(())
    })
    .await;

    rt.idle().await;

    let _ = r.map_err(|r| eprintln!("{}", r));

    Ok(())
}
