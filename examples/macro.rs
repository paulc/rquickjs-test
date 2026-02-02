use anyhow::{anyhow, Result};
use rquickjs::{CatchResultExt, Context, Runtime};

#[rquickjs::module(rename_vars = "camelCase")]
mod native_api {

    use rquickjs::{Ctx, Exception, Object};

    #[rquickjs::function]
    pub fn fetch_text(url: String) -> Result<String, rquickjs::Error> {
        let body = reqwest::blocking::get(url)
            .map_err(|_e| rquickjs::Error::Exception)?
            .text()
            .map_err(|_e| rquickjs::Error::Exception)?;
        Ok(body)
    }

    #[rquickjs::function]
    pub fn zark<'js>(ctx: Ctx<'js>) -> Result<Object<'js>, rquickjs::Error> {
        let object = Object::new(ctx.clone())?;
        object.set("zark", "ZARK")?;
        Ok(object.into())
    }

    #[rquickjs::function]
    pub fn throw_ex(ctx: rquickjs::Ctx<'_>, msg: String) -> Result<String, rquickjs::Error> {
        Err(Exception::throw_message(&ctx, &msg))
    }
}

#[rquickjs::function]
pub fn print(s: String) {
    println!("{s}");
}

fn main() -> Result<()> {
    let rt = Runtime::new()?;
    let ctx = Context::full(&rt)?;

    let script = r#"
            import {fetch_text, zark, throw_ex} from "native_api";

            print("START");

            print(JSON.stringify(zark()));

            try {
              throw_ex("CATCH");
            } catch(e) {
              print(`Exception: ${e.message}`);
            }

            throw_ex("BYE");

            print("END");
    "#;

    let r = ctx.with::<_, Result<()>>(|ctx| {
        rquickjs::Module::declare_def::<js_native_api, _>(ctx.clone(), "native_api").unwrap();
        ctx.globals().set("print", js_print).unwrap();

        let m = rquickjs::Module::declare(ctx.clone(), "script", script)
            .catch(&ctx)
            .map_err(|e| anyhow!("JS error [declare]: {}", e))?;
        let (_m, m_promise) = m
            .eval()
            .catch(&ctx)
            .map_err(|e| anyhow!("JS error [eval]: {}", e))?;
        () = m_promise
            .finish()
            .catch(&ctx)
            .map_err(|e| anyhow!("JS error [finish]: {}", e))?;
        Ok(())
    });

    let _ = r.map_err(|r| eprintln!("{}", r));

    Ok(())
}
