use anyhow::{anyhow, Result};
use rquickjs::class::Trace;
use rquickjs::{ArrayBuffer, CatchResultExt, Class, Context, Ctx, JsLifetime, Runtime, Value};

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
fn print_v<'js>(ctx: Ctx<'js>, v: Value<'js>) -> rquickjs::Result<()> {
    let output = ctx
        .json_stringify(v)?
        .and_then(|s| s.as_string().map(|s| s.to_string().ok()).flatten())
        .unwrap_or_else(|| "<ERR>".to_string());
    println!("{}", output);
    Ok(())
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

            const o = { a:1, b:[1,2,3], c:false };
            print_v(e.get_type());
            print_v(t.data.byteLength);
            print_v(t.text);
            const v = new Int8Array(t.data);
            print_v(v);
            t.text = "CHANGED";
            print(t.format());

            throw_ex("BYE");

            print("END");
    "#;

    let r = ctx.with::<_, Result<()>>(|ctx| {
        rquickjs::Module::declare_def::<js_native_api, _>(ctx.clone(), "native_api").unwrap();
        ctx.globals().set("print", js_print).unwrap();
        ctx.globals().set("print_v", js_print_v).unwrap();

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
